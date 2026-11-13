"""Legacy compatibility module."""
from github_engine import *
if __name__=="__main__":
    from github_engine import main
    raise SystemExit(main())
