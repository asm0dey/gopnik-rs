// Address annotation: a line that machine code contributed to ends in a C++
// comment, a space, an at-sign, a space, then EVERY contributing address in
// Ghidra segmented form -- not the line's minimum address, which would name
// one instruction for a line that merges several.
// Form A: file_off = 0x18d0 + (SEG - 0x1000) * 16 + OFF.
// This is a LEAD, not evidence: verify each citation with
// `python3 tools/re_query.py resolve <citation>` before writing it down.

int __stdcall16far FUN_1f78_114b(uint param_1)

{
  long lVar1;
  ulong uVar2;
  
  uVar2 = FUN_1f78_11a8();                                          // @ 2000:08cb
  lVar1 = (uVar2 >> 0x10) * (ulong)param_1;                         // @ 2000:08cb 2000:08da
  return (int)((ulong)lVar1 >> 0x10) +                              // @ 2000:08da 2000:08e0 2000:08e5
         (uint)CARRY2((uint)lVar1,(uint)((uVar2 & 0xffff) * (ulong)param_1 >> 0x10));  // @ 2000:08d2 2000:08da 2000:08de
}

