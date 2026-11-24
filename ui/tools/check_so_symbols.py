#!/usr/bin/env python3
"""Fail when an Android shared library has undefined symbols that nothing will provide.

A cdylib is linked without -z defs by default, so a symbol that no linked library
defines (such as libc++'s std::__ndk1::__hash_memory, missing from an older NDK)
links fine and only fails at dlopen on the device. This check reads the library's
DT_NEEDED entries and resolves every strong undefined symbol against the NDK
sysroot stubs for the given API level, and against any library in --extra-dir
(the other .so files shipped beside it in jniLibs).

Usage:
  check_so_symbols.py --sysroot-lib <ndk>/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android \
      --api 24 [--extra-dir jniLibs/arm64-v8a] lib.so [lib.so ...]

Needs pyelftools.
"""

import argparse
import os
import sys

from elftools.elf.dynamic import DynamicSection
from elftools.elf.elffile import ELFFile
from elftools.elf.sections import SymbolTableSection


def dynsyms(path):
    with open(path, "rb") as f:
        elf = ELFFile(f)
        needed = []
        for sec in elf.iter_sections():
            if isinstance(sec, DynamicSection):
                needed = [t.needed for t in sec.iter_tags() if t.entry.d_tag == "DT_NEEDED"]
        table = elf.get_section_by_name(".dynsym")
        defined, undefined = set(), set()
        if isinstance(table, SymbolTableSection):
            for sym in table.iter_symbols():
                if not sym.name:
                    continue
                if sym["st_shndx"] == "SHN_UNDEF":
                    if sym["st_info"]["bind"] != "STB_WEAK":
                        undefined.add(sym.name)
                else:
                    defined.add(sym.name)
        return needed, defined, undefined


def find_lib(name, search):
    for d in search:
        p = os.path.join(d, name)
        if os.path.isfile(p):
            return p
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--sysroot-lib", required=True)
    ap.add_argument("--api", required=True)
    ap.add_argument("--extra-dir", action="append", default=[])
    ap.add_argument("libs", nargs="+")
    args = ap.parse_args()

    search = args.extra_dir + [os.path.join(args.sysroot_lib, args.api), args.sysroot_lib]
    failed = False
    for lib in args.libs:
        needed, _, undefined = dynsyms(lib)
        provided = set()
        missing_libs = []
        for name in needed:
            path = find_lib(name, search)
            if path is None:
                missing_libs.append(name)
                continue
            provided |= dynsyms(path)[1]
        unresolved = sorted(undefined - provided)
        print(f"{lib}: NEEDED {', '.join(needed)}; {len(undefined)} undefined, {len(unresolved)} unresolved")
        if missing_libs:
            failed = True
            print(f"::error::{lib}: NEEDED libraries not in the NDK sysroot or beside it: {', '.join(missing_libs)}")
        if unresolved:
            failed = True
            print(f"::error::{lib}: {len(unresolved)} undefined symbols no NEEDED library defines")
            for s in unresolved:
                print(f"  {s}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
