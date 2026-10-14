/*
 * x_info.h
 *
 * Usage info
 *
 */

#ifndef X_INFO_H
#define X_INFO_H

#define INFORMATION "\
  -aa   watch attribute setting   \t -P   alter piracy opcode\n\
  -at   watch attribute testing   \t -rm # right margin\n\
  -bg <colorname> background color\t -rs # random number seed value\n\
  -c # context lines              \t -sc # transcript width\n\
  -fg <colorname> foreground color\t -t   set Tandy bit\n\
  -fn <fontname> set font name    \t -u # slots for multiple undo\n\
  -i   ignore fatal errors        \t -v   show version information\n\
  -lm # left margin               \t -w # text width\n\
  -ol   watch object movement     \t -x   expand abbreviations g/x/z\n\
  -L  <file> load this save file  \t -xrm <resources> Set X11 resources\n\
  -om   watch object locating     \t -zs # error checking (see below)\n"

#define INFO2 "\
Error checking: 0 none, 1 first only (default), 2 all, 3 exit after any error.\n\
For more options and explanations, please read the manual page.\n"

#endif
