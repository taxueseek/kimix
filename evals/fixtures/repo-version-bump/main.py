import sys
from src.app import version, greet

if __name__ == "__main__":
    print(f"version={version()}")
    print(greet(sys.argv[1] if len(sys.argv) > 1 else "Kimix"))
