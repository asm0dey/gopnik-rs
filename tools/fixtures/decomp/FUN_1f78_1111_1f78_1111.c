// Address annotation: a line that machine code contributed to ends in a C++
// comment, a space, an at-sign, a space, then EVERY contributing address in
// Ghidra segmented form -- not the line's minimum address, which would name
// one instruction for a line that merges several.
// Form A: file_off = 0x18d0 + (SEG - 0x1000) * 16 + OFF.
// This is a LEAD, not evidence: verify each citation with
// `python3 tools/re_query.py resolve <citation>` before writing it down.

/* WARNING: Control flow encountered bad instruction data */        // @ 2000:0891

void __cdecl16far FUN_1f78_1111(void)

{
  undefined1 in_CF;
  
  FUN_1f78_0eb1();                                                  // @ 2000:0891
  if (!(bool)in_CF) {                                               // @ 2000:0894
    return;                                                         // @ 2000:0896
  }
                    /* WARNING: Bad instruction - Truncating control flow here */  // @ 2000:f88f
  halt_baddata();                                                   // @ 2000:f88f
}

