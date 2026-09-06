// Address annotation: a line that machine code contributed to ends in a C++
// comment, a space, an at-sign, a space, then EVERY contributing address in
// Ghidra segmented form -- not the line's minimum address, which would name
// one instruction for a line that merges several.
// Form A: file_off = 0x18d0 + (SEG - 0x1000) * 16 + OFF.
// This is a LEAD, not evidence: verify each citation with
// `python3 tools/re_query.py resolve <citation>` before writing it down.

void FUN_1000_11c2(char param_1)

{
  undefined2 unaff_DS;
  
  FUN_1f78_02cd();                                                  // @ 1000:11c8
  *(undefined2 *)0x3952 = 10;                                       // @ 1000:11d0
  if (param_1 == '\0') {                                            // @ 1000:11d6 1000:11da
    *(undefined2 *)0x395c = 0x7d;                                   // @ 1000:11dc
    *(undefined2 *)0x3954 = 0x29;                                   // @ 1000:11e2
    *(undefined2 *)0x3956 = 0x32;                                   // @ 1000:11e8
    *(undefined2 *)0x3958 = 0x7b;                                   // @ 1000:11ee
    *(undefined2 *)0x395a = 0x24;                                   // @ 1000:11f4
    *(undefined1 *)0x3968 = 0x3c;                                   // @ 1000:11fa
  }
  if (param_1 == '\x01') {                                          // @ 1000:11ff 1000:1203
    *(undefined2 *)0x395c = 0xa0;                                   // @ 1000:1205
    *(undefined2 *)0x3954 = 0x32;                                   // @ 1000:120b
    *(undefined2 *)0x3956 = 0x3c;                                   // @ 1000:1211
    *(undefined2 *)0x3958 = 0xbc;                                   // @ 1000:1217
    *(undefined2 *)0x395a = 0x20;                                   // @ 1000:121d
    *(undefined1 *)0x3968 = 0x50;                                   // @ 1000:1223
  }
  *(int *)0x395e = *(int *)0x3954 / 2;                              // @ 1000:1228 1000:122f 1000:1231
  *(undefined2 *)0x3960 = *(undefined2 *)0x3954;                    // @ 1000:1234 1000:1237
  *(int *)0x3964 = *(int *)0x3958 * 5 + *(int *)0x3954 + 10;        // @ 1000:123a 1000:1243 1000:1245 1000:1249 1000:124c
  *(undefined2 *)0x3962 = *(undefined2 *)0x3964;                    // @ 1000:124f 1000:1252
  *(undefined1 *)0x3966 = 0;                                        // @ 1000:1255
  *(undefined1 *)0x3967 = 0;                                        // @ 1000:125a
  *(undefined2 *)0x396c = 0;                                        // @ 1000:1261
  *(undefined2 *)0x396a = 0;                                        // @ 1000:1266
  *(undefined2 *)0x396e = 0;                                        // @ 1000:126b
  return;                                                           // @ 1000:1271
}

