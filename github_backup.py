"""Legacy compatibility import; use multi_backup.py."""
from multi_backup import *
if __name__=="__main__":
    import sys
    sys.exit(main())
