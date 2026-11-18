// Multi Backup r218: macOS native iCloud Drive File Provider bridge.
// No Apple ID credentials, private web endpoints or fake CloudKit-wide Drive API.
import Foundation
import CryptoKit

enum BridgeError: Error, CustomStringConvertible {
    case invalid(String)
    var description: String {
        switch self { case .invalid(let why): return why }
    }
}
func guardRelative(_ value: String) throws -> [String] {
    if value.isEmpty || value.count > 1024 || value.hasPrefix("/") ||
       value.contains("\\") || value.contains("\0") || value.contains("\n") ||
       value.contains("\r") { throw BridgeError.invalid("Unsafe relative path") }
    let components = value.split(separator:"/", omittingEmptySubsequences:false).map(String.init)
    if components.contains(where: { $0.isEmpty || $0 == "." || $0 == ".." ||
                                    $0.hasPrefix("~") || $0.contains(":") }) {
        throw BridgeError.invalid("Relative path has unsafe segment")
    }
    return components
}
func cloudRoot() throws -> URL {
    let url = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent("Library/Mobile Documents/com~apple~CloudDocs", isDirectory:true)
    var isDir: ObjCBool = false
    guard FileManager.default.fileExists(atPath:url.path,isDirectory:&isDir),isDir.boolValue else {
        throw BridgeError.invalid("The signed-in macOS iCloud Drive local File Provider is unavailable")
    }
    let props = try FileManager.default.attributesOfItem(atPath:url.path)
    if props[.type] as? FileAttributeType == .typeSymbolicLink {
        throw BridgeError.invalid("Symlinked iCloud root refused")
    }
    return url.standardizedFileURL
}
func cloudItem(_ relative: String) throws -> URL {
    let parts = try guardRelative(relative)
    let root = try cloudRoot()
    var parent = root
    for segment in parts.dropLast() {
        parent.appendPathComponent(segment, isDirectory: true)
        let props = try FileManager.default.attributesOfItem(atPath:parent.path)
        guard props[.type] as? FileAttributeType == .typeDirectory else {
            throw BridgeError.invalid("Cloud parent must be an actual directory; symlink refused")
        }
    }
    return parent.appendingPathComponent(parts.last!,isDirectory:false)
}
func regular(_ file: URL) throws {
    let props = try FileManager.default.attributesOfItem(atPath:file.path)
    guard props[.type] as? FileAttributeType == .typeRegular else {
        throw BridgeError.invalid("Regular file required; symlink, placeholder or directory refused")
    }
}
func sha256(_ file: URL) throws -> (String, UInt64) {
    let stream = try FileHandle(forReadingFrom:file)
    defer { try? stream.close() }
    var hash = SHA256()
    var total: UInt64 = 0
    while true {
        let part = try stream.read(upToCount: 1024*1024) ?? Data()
        if part.isEmpty { break }
        total += UInt64(part.count)
        hash.update(data:part)
    }
    return (hash.finalize().map { String(format:"%02x",$0) }.joined(),total)
}
func output(_ object: [String:Any]) throws {
    let data=try JSONSerialization.data(withJSONObject:object,options:[.sortedKeys])
    guard let line=String(data:data,encoding:.utf8) else { throw BridgeError.invalid("JSON UTF8 encoding") }
    print(line)
}
func checkedTarget(_ url: URL) throws {
    if FileManager.default.fileExists(atPath:url.path) {
        throw BridgeError.invalid("Create-only destination exists; overwrite refused")
    }
    let folder=url.deletingLastPathComponent()
    let props=try FileManager.default.attributesOfItem(atPath:folder.path)
    guard props[.type] as? FileAttributeType == .typeDirectory else {
        throw BridgeError.invalid("Target parent must be an actual directory, not symlink")
    }
}
func copyNew(_ source: URL, to destination:URL) throws {
    try checkedTarget(destination)
    try regular(source)
    try FileManager.default.copyItem(at:source,to:destination) // no overwrite
}
func coordinatedPull(relative:String, destination:String) throws {
    let source=try cloudItem(relative)
    let target=URL(fileURLWithPath:destination).standardizedFileURL
    try checkedTarget(target)
    let coordinator=NSFileCoordinator(filePresenter:nil)
    var coordinatorError:NSError?
    var copyError:Error?
    coordinator.coordinate(readingItemAt:source,options:[],error:&coordinatorError) { readable in
        do { try copyNew(readable,to:target) } catch { copyError=error }
    }
    if let e=coordinatorError { throw e }
    if let e=copyError { throw e }
    let (digest,size)=try sha256(target)
    try output(["operation":"icloud-pull","source":relative,"bytes":size,
                "sha256":digest,"local_copy":target.path,"icloud_server_receipt":false])
}
func ageCiphertext(_ source:URL) throws {
    try regular(source)
    guard source.lastPathComponent.hasSuffix(".age") else {
        throw BridgeError.invalid("iCloud output accepts only .age encrypted archive")
    }
    let f=try FileHandle(forReadingFrom:source)
    defer { try? f.close() }
    let bytes=try f.read(upToCount:48) ?? Data()
    let header=Data("age-encryption.org/v1\n".utf8)
    guard bytes.starts(with:header) else {
        throw BridgeError.invalid("Not a native age ciphertext header; validate/decrypt locally first")
    }
}
func coordinatedPush(archive:String, relative:String) throws {
    let source=URL(fileURLWithPath:archive).standardizedFileURL
    let target=try cloudItem(relative)
    guard relative.hasSuffix(".age") else {
        throw BridgeError.invalid("Remote iCloud archive must end in .age")
    }
    try ageCiphertext(source)
    try checkedTarget(target)
    let (before,sourceBytes)=try sha256(source)
    let coordinator=NSFileCoordinator(filePresenter:nil)
    var coordinatorError:NSError?
    var copyError:Error?
    coordinator.coordinate(writingItemAt:target,options:[],error:&coordinatorError) { writable in
        do { try copyNew(source,to:writable) } catch { copyError=error }
    }
    if let e=coordinatorError { throw e }
    if let e=copyError { throw e }
    // Read-back checks byte integrity of *local coordinated File Provider copy*.
    // iCloud upload completion on other Apple devices is NOT proven.
    let (after,targetBytes)=try sha256(target)
    guard before==after && sourceBytes==targetBytes else {
        throw BridgeError.invalid("iCloud local File Provider copy checksum mismatch")
    }
    try output(["operation":"icloud-push","destination":relative,"bytes":targetBytes,
                "sha256":after,"create_only":true,"icloud_server_receipt":false])
}
func run() throws {
    let args=Array(CommandLine.arguments.dropFirst())
    if args==["--self-test"] {
        for bad in ["../evil","/etc/passwd","good//bad","a/./b","ok/../escape",
                    "slash\\x","folder/~hidden","bad:\0",""] {
            guard (try? guardRelative(bad))==nil else {
                throw BridgeError.invalid("Self-test accepted unsafe relative path")
            }
        }
        guard try guardRelative("Backups/backup-2026.tar.gz.age").count==2 else {
            throw BridgeError.invalid("Self-test rejected good backup relative path")
        }
        try output(["self_test":"PASS","macos_file_provider_network_certified":false])
        return
    }
    guard args.count==3 else {
        throw BridgeError.invalid("Usage: multi-backup-icloud pull RELATIVE_ICLOUD_FILE NEW_LOCAL_FILE | push LOCAL_VERIFIED.age NEW_RELATIVE_ICLOUD.age")
    }
    switch args[0] {
    case "pull": try coordinatedPull(relative:args[1],destination:args[2])
    case "push": try coordinatedPush(archive:args[1],relative:args[2])
    default: throw BridgeError.invalid("Unknown command")
    }
}
do { try run() }
catch { fputs("REFUSED iCloud provider: \(error)\n",stderr); exit(2) }
