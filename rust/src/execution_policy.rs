//! Shared server/agent execution budget, used for all input plugins.
//! A policy may only be tightened at the host; a push agent cannot bypass
//! server ceilings. CPU and RAM thresholds are cooperative admission checks,
//! NOT substitutes for cgroups/systemd/Windows Job Objects resource fencing.
use serde::{Deserialize,Serialize};
use std::{fs::{self,OpenOptions},io::{self,Read,Write,Seek,SeekFrom},
    path::{Path,PathBuf},time::{Duration,Instant,SystemTime,UNIX_EPOCH},thread};
use sysinfo::System;

#[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionLimits {
    #[serde(default="default_jobs")] pub max_concurrent_jobs:u16,
    #[serde(default="default_threads")] pub threads_per_job:u16,
    #[serde(default)] pub max_transfer_bytes_per_sec:Option<u64>,
    #[serde(default)] pub max_host_cpu_percent:Option<f32>,
    #[serde(default)] pub max_host_memory_percent:Option<f32>,
    #[serde(default)] pub min_free_memory_bytes:Option<u64>,
}
fn default_jobs()->u16{4}
fn default_threads()->u16{4}
impl Default for ExecutionLimits {
    fn default()->Self{Self{max_concurrent_jobs:default_jobs(),threads_per_job:default_threads(),
        max_transfer_bytes_per_sec:None,max_host_cpu_percent:None,
        max_host_memory_percent:None,min_free_memory_bytes:None}}
}
impl ExecutionLimits {
    pub fn validate(&self)->Result<(),String>{
        if !(1..=256).contains(&self.max_concurrent_jobs)||
            !(1..=64).contains(&self.threads_per_job){
            return Err("Parallelism must be 1..256 jobs, 1..64 threads per job".into());
        }
        if self.max_concurrent_jobs as usize*self.threads_per_job as usize>2048 {
            return Err("Maximum total worker count is 2048".into());
        }
        if self.max_transfer_bytes_per_sec.is_some_and(|v|v<1024){
            return Err("Transfer cap must be >=1024 bytes/s".into());
        }
        for value in [self.max_host_cpu_percent,self.max_host_memory_percent]{
            if value.is_some_and(|v| !v.is_finite()||v<=0.0||v>100.0){
                return Err("Host CPU/memory percent must be in (0,100]".into());
            }
        }
        Ok(())
    }
    pub fn strictest(server:&Self,agent:&Self)->Result<Self,String>{
        server.validate()?;agent.validate()?;
        fn min_opt<T:Ord+Copy>(a:Option<T>,b:Option<T>)->Option<T>{
            match (a,b){(Some(x),Some(y))=>Some(x.min(y)),(Some(x),None)|
                (None,Some(x))=>Some(x),(None,None)=>None}
        }
        fn min_float(a:Option<f32>,b:Option<f32>)->Option<f32>{
            match (a,b){(Some(x),Some(y))=>Some(x.min(y)),(Some(x),None)|
                (None,Some(x))=>Some(x),(None,None)=>None}
        }
        let effective=Self {
            max_concurrent_jobs:server.max_concurrent_jobs.min(agent.max_concurrent_jobs),
            threads_per_job:server.threads_per_job.min(agent.threads_per_job),
            max_transfer_bytes_per_sec:min_opt(server.max_transfer_bytes_per_sec,
                                               agent.max_transfer_bytes_per_sec),
            max_host_cpu_percent:min_float(server.max_host_cpu_percent,
                                           agent.max_host_cpu_percent),
            max_host_memory_percent:min_float(server.max_host_memory_percent,
                                              agent.max_host_memory_percent),
            // Minimum free memory is a floor, so choose the stricter maximum.
            min_free_memory_bytes:match (server.min_free_memory_bytes,
                                          agent.min_free_memory_bytes){
                (Some(a),Some(b))=>Some(a.max(b)),(a,b)=>a.or(b),
            },
        };
        effective.validate()?;
        Ok(effective)
    }
    pub fn check_host(&self)->Result<(),String>{
        self.validate()?;
        if self.max_host_memory_percent.is_none()&&self.min_free_memory_bytes.is_none()&&
           self.max_host_cpu_percent.is_none(){return Ok(());}
        let mut sys=System::new();
        sys.refresh_memory();
        let total=sys.total_memory();
        let free=sys.available_memory();
        if total==0{return Err("Host memory probe unavailable; limit cannot be verified".into());}
        if let Some(percentage)=self.max_host_memory_percent{
            if (total-free) as f64*100.0/total as f64 >percentage as f64{
                return Err("Host RAM usage above backup admission threshold".into());
            }
        }
        if let Some(limit)=self.min_free_memory_bytes{
            if free<limit{return Err("Host free memory below backup reserve".into());}
        }
        if let Some(limit)=self.max_host_cpu_percent{
            sys.refresh_cpu_usage();
            thread::sleep(Duration::from_millis(250));
            sys.refresh_cpu_usage();
            if sys.cpus().is_empty(){
                return Err("CPU telemetry unavailable; threshold cannot be checked".into());
            }
            if sys.global_cpu_info().cpu_usage()>limit {
                return Err("Host CPU usage above backup admission threshold".into());
            }
        }
        Ok(())
    }
}
/// Streaming per-transfer bandwidth cap (also for SSH dumps and agent push).
/// No unbounded buffering; host threshold checked periodically while writing.
pub struct ThrottledWriter<W:Write>{
    inner:W,rate:Option<u64>,window:Instant,window_bytes:u64,
}
impl<W:Write> ThrottledWriter<W>{
    pub fn new(inner:W,limits:&ExecutionLimits)->Result<Self,String>{
        limits.validate()?;
        Ok(Self{inner,rate:limits.max_transfer_bytes_per_sec,
            window:Instant::now(),window_bytes:0})
    }
    pub fn into_inner(self)->W{self.inner}
}
impl<W:Write> Write for ThrottledWriter<W>{
    fn write(&mut self,buf:&[u8])->io::Result<usize>{
        if buf.is_empty(){return Ok(0);}
        let max=self.rate.map(|r|(r/10).max(1024) as usize)
            .unwrap_or(buf.len()).min(buf.len());
        if let Some(rate)=self.rate{
            let now=Instant::now();
            if now.duration_since(self.window)>=Duration::from_secs(1){
                self.window=now;self.window_bytes=0;
            }
            let allowance=rate.saturating_sub(self.window_bytes);
            if allowance==0 {
                let remaining=Duration::from_secs(1).saturating_sub(self.window.elapsed());
                thread::sleep(remaining);
                self.window=Instant::now();
                self.window_bytes=0;
            }
        }
        let allowed=self.rate.map(|r|r.saturating_sub(self.window_bytes)
            .max(1).min(max as u64) as usize).unwrap_or(max);
        let n=self.inner.write(&buf[..allowed])?;
        self.window_bytes=self.window_bytes.saturating_add(n as u64);
        Ok(n)
    }
    fn flush(&mut self)->io::Result<()>{self.inner.flush()}
}
/// Agent-host-wide cooperative upload limit shared by independent agent
/// processes through an owner-private, locked accounting file. Fixed
/// one-second UTC windows deliberately permit no more than the configured
/// byte count per window. This is LOCAL to one host: it is not a multi-node
/// quota or OS-level network/cgroup enforcement.
#[derive(Debug)]
pub struct SharedHostBudget {
    state_file:PathBuf,
    rate:u64,
}
#[derive(Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct HostBudgetState {
    epoch_second:u64,
    reserved_bytes:u64,
}
impl SharedHostBudget {
    pub fn new(private_directory:&Path,rate:u64)->io::Result<Self>{
        if rate<1024{return Err(io::Error::other("Host upload cap must be >= 1024 bytes/s"));}
        let metadata=fs::symlink_metadata(private_directory)?;
        if !metadata.file_type().is_dir(){
            return Err(io::Error::other("Host budget directory must be a real directory"));
        }
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode()&0o077!=0{
                return Err(io::Error::other("Host budget directory must be private (0700)"));
            }
        }
        Ok(Self{state_file:private_directory.join(".host-upload-budget-v1.json"),rate})
    }
    /// Reserve before reading from disk; failed/short reads may underutilize
    /// bandwidth but can never silently raise the aggregate transfer ceiling.
    pub fn reserve(&self,wanted:usize)->io::Result<usize>{
        if wanted==0{return Ok(0);}
        loop {
            let mut options=OpenOptions::new();
            options.read(true).write(true).create(true);
            #[cfg(unix)] {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file=options.open(&self.state_file)?;
            let metadata=fs::symlink_metadata(&self.state_file)?;
            if !metadata.file_type().is_file(){
                return Err(io::Error::other("Host budget file must not be a symlink"));
            }
            #[cfg(unix)] {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode()&0o077!=0{
                    return Err(io::Error::other("Host budget file must be private (0600)"));
                }
            }
            fs2::FileExt::lock_exclusive(&file)?;
            // Always unlock, including on malformed/partially written state.
            let checked=(||->io::Result<Option<usize>> {
                file.seek(SeekFrom::Start(0))?;
                let mut bytes=Vec::new();
                (&mut file).take(257).read_to_end(&mut bytes)?;
                if bytes.len()>256{
                    return Err(io::Error::other("Host budget state is oversized"));
                }
                let now=SystemTime::now().duration_since(UNIX_EPOCH)
                    .map_err(|_|io::Error::other("System clock predates Unix epoch"))?;
                let mut state:HostBudgetState=if bytes.is_empty(){
                    HostBudgetState{epoch_second:now.as_secs(),reserved_bytes:0}
                }else{
                    serde_json::from_slice(&bytes).map_err(|_|
                        io::Error::other("Corrupted host budget state: operator repair required"))?
                };
                if state.epoch_second>now.as_secs() || state.reserved_bytes>self.rate{
                    return Err(io::Error::other(
                        "Invalid host budget counter or clock rollback: failing closed"));
                }
                if state.epoch_second<now.as_secs(){
                    state.epoch_second=now.as_secs();
                    state.reserved_bytes=0;
                }
                let available=self.rate-state.reserved_bytes;
                if available==0{return Ok(None);}
                let count=(wanted as u64).min(available).min(self.rate)
                    .min(128*1024) as usize;
                state.reserved_bytes+=count as u64;
                file.set_len(0)?;
                file.seek(SeekFrom::Start(0))?;
                serde_json::to_writer(&mut file,&state).map_err(io::Error::other)?;
                file.sync_data()?;
                Ok(Some(count))
            })();
            let unlock=fs2::FileExt::unlock(&file);
            unlock?;
            match checked? {
                Some(count)=>return Ok(count),
                None=>{
                    let now=SystemTime::now().duration_since(UNIX_EPOCH)
                        .map_err(|_|io::Error::other("Invalid system clock"))?;
                    let rest=Duration::from_secs(1)
                        .saturating_sub(Duration::from_nanos(now.subsec_nanos() as u64));
                    thread::sleep(rest.max(Duration::from_millis(1)));
                }
            }
        }
    }
}

pub struct ThrottledReader<R:Read>{
    inner:R,limits:ExecutionLimits,window:Instant,window_bytes:u64,
    last_host_check:Instant,
    host_budget:Option<SharedHostBudget>,
}
impl<R:Read> ThrottledReader<R>{
    pub fn new(inner:R,limits:ExecutionLimits)->Result<Self,String>{
        limits.validate()?;
        Ok(Self{inner,limits,window:Instant::now(),window_bytes:0,
            last_host_check:Instant::now()-Duration::from_secs(2),
            host_budget:None})
    }
}
impl<R:Read> ThrottledReader<R>{
    /// Attach an explicit host-local cap; never trust a network-supplied path.
    pub fn with_host_upload_budget(mut self,private_dir:&Path,rate:u64)
        ->io::Result<Self>{
        self.host_budget=Some(SharedHostBudget::new(private_dir,rate)?);
        Ok(self)
    }
}
impl<R:Read> Read for ThrottledReader<R>{
    fn read(&mut self,buf:&mut [u8])->io::Result<usize>{
        if self.last_host_check.elapsed()>Duration::from_secs(1){
            self.limits.check_host().map_err(io::Error::other)?;
            self.last_host_check=Instant::now();
        }
        if buf.is_empty(){return Ok(0);}
        let length=if let Some(rate)=self.limits.max_transfer_bytes_per_sec{
            if self.window.elapsed()>=Duration::from_secs(1){
                self.window=Instant::now();self.window_bytes=0;
            }
            if self.window_bytes>=rate{
                thread::sleep(Duration::from_secs(1).saturating_sub(self.window.elapsed()));
                self.window=Instant::now();self.window_bytes=0;
            }
            buf.len().min(rate.saturating_sub(self.window_bytes).max(1) as usize)
        }else{buf.len()};
        let allowed=match &self.host_budget {
            Some(budget)=>budget.reserve(length)?,
            None=>length,
        };
        let n=self.inner.read(&mut buf[..allowed])?;
        self.window_bytes=self.window_bytes.saturating_add(n as u64);
        Ok(n)
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    fn private_tempdir()->tempfile::TempDir{
        let directory=tempfile::tempdir().unwrap();
        #[cfg(unix)]{
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory.path(),
                fs::Permissions::from_mode(0o700)).unwrap();
        }
        directory
    }
    #[test]fn limits_are_intersected_not_relaxed(){
        let server=ExecutionLimits{max_concurrent_jobs:4,threads_per_job:8,
            max_transfer_bytes_per_sec:Some(1024*1024),
            max_host_cpu_percent:Some(75.0),max_host_memory_percent:Some(80.0),
            min_free_memory_bytes:Some(512*1024*1024)};
        let agent=ExecutionLimits{max_concurrent_jobs:16,threads_per_job:4,
            max_transfer_bytes_per_sec:Some(4*1024*1024),
            max_host_cpu_percent:Some(95.0),max_host_memory_percent:Some(65.0),
            min_free_memory_bytes:Some(128*1024*1024)};
        let l=ExecutionLimits::strictest(&server,&agent).unwrap();
        assert_eq!(l.max_concurrent_jobs,4);assert_eq!(l.threads_per_job,4);
        assert_eq!(l.max_transfer_bytes_per_sec,Some(1024*1024));
        assert_eq!(l.max_host_cpu_percent,Some(75.0));
        assert_eq!(l.max_host_memory_percent,Some(65.0));
        assert_eq!(l.min_free_memory_bytes,Some(512*1024*1024));
    }
    #[test]fn deny_invalid_resource_limits(){
        let mut l=ExecutionLimits::default();
        l.max_host_cpu_percent=Some(f32::NAN);assert!(l.validate().is_err());
        l.max_host_cpu_percent=None;l.max_concurrent_jobs=0;
        assert!(l.validate().is_err());
        l.max_concurrent_jobs=4;l.max_transfer_bytes_per_sec=Some(10);
        assert!(l.validate().is_err());
    }
    #[test]fn throttled_stream_preserves_contents(){
        let l=ExecutionLimits{max_transfer_bytes_per_sec:Some(65536),..Default::default()};
        let mut w=ThrottledWriter::new(Vec::<u8>::new(),&l).unwrap();
        let buf=vec![0x42;8192];
        w.write_all(&buf).unwrap();assert_eq!(w.into_inner(),buf);
        let mut r=ThrottledReader::new(&buf[..],l).unwrap();
        let mut readback=Vec::new();r.read_to_end(&mut readback).unwrap();
        assert_eq!(readback,buf);
    }

    #[test]fn independent_readers_share_same_host_budget_file(){
        let directory=private_tempdir();
        let limits=ExecutionLimits::default();
        let mut a=ThrottledReader::new(&b"abcde"[..],limits.clone()).unwrap()
            .with_host_upload_budget(directory.path(),8192).unwrap();
        let mut b=ThrottledReader::new(&b"vwxyz"[..],limits).unwrap()
            .with_host_upload_budget(directory.path(),8192).unwrap();
        let mut left=[0u8;5];
        let mut right=[0u8;5];
        assert_eq!(a.read(&mut left).unwrap(),5);
        assert_eq!(b.read(&mut right).unwrap(),5);
        assert_eq!(&left,b"abcde");
        assert_eq!(&right,b"vwxyz");
        let stored=fs::read(directory.path().join(".host-upload-budget-v1.json")).unwrap();
        let state:HostBudgetState=serde_json::from_slice(&stored).unwrap();
        assert_eq!(state.reserved_bytes,10);
    }
    #[test]fn malformed_or_future_host_budget_fails_closed(){
        let directory=private_tempdir();
        let budget=SharedHostBudget::new(directory.path(),2048).unwrap();
        fs::write(&budget.state_file,b"not valid json").unwrap();
        assert!(budget.reserve(1).is_err());
        fs::write(&budget.state_file,
            serde_json::to_vec(&HostBudgetState{
                epoch_second:u64::MAX,reserved_bytes:1}).unwrap()).unwrap();
        assert!(budget.reserve(1).is_err());
        assert!(SharedHostBudget::new(directory.path(),100).is_err());
    }
    #[cfg(unix)]
    #[test]fn reject_symlink_host_budget_state(){
        use std::os::unix::fs::symlink;
        let directory=private_tempdir();
        let victim=directory.path().join("victim");
        fs::write(&victim,b"unchanged").unwrap();
        let link=directory.path().join(".host-upload-budget-v1.json");
        symlink(&victim,&link).unwrap();
        let budget=SharedHostBudget::new(directory.path(),4096).unwrap();
        assert!(budget.reserve(1).is_err());
        assert_eq!(fs::read(&victim).unwrap(),b"unchanged");
    }
}
