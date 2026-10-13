
! Prints out the name of a character. This also prints a warning
! if the character should not be accepted by a particular input
! method (given in restr.)
! restr = 1 for by-line, 2 for by-char, 0 for no warning
[ DescChar ch restr
  res;

  res = 0;
  ! 0 for ok everywhere, -1 for never ok, 2 for ok only
  ! in read_char

  if (ch == 8) {
    print "control character 'ctrl-H' (z-machine 'delete')";
    res = 2;
  }
  else if (ch == 13) {
    print "control character 'ctrl-M' (z-machine 'newline')";
    res = 2;
  }
  else if (ch == 27) {
    print "control character 'ctrl-[' (z-machine 'escape')";
    res = 2;
  }
  else if (ch < 32) {
    print "control character 'ctrl-", (char)(ch+64), "'";
    res = -1;
  }
  else if (ch == 127) {
    print "control character 'delete'";
    res = -1;
  }
  else if (ch == 128) {
    print "undefined character";
    res = -1;
  }
  else if (ch < 128) {
    print "ASCII character '", (char)ch, "'";
  }
  else if (ch < 155) {
    res = 2;
    switch (ch) {
     129: print "cursor up";
     130: print "cursor down";
     131: print "cursor left";
     132: print "cursor right";
     133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143, 144:
      print "function key ", ch-132;
     145, 146, 147, 148, 149, 150, 151, 152, 153, 154:
      print "keypad key ", ch-145;
    }
  }
  else if (ch < 224) {
    print (string)AccentList-->(ch - 155 + 1);
  }
  else if (ch < 252) {
    print "undefined character";
    res = -1;
  }
  else if (ch < 255) {
    res = 2;
    switch (ch) {
     252: print "menu click";
     253: print "mouse double-click";
     254: print "mouse single-click";
    }
  }
  else {
    res = -1;
    print "undefined character";
  }

  if (restr == 1) {
    switch (res) {
     -1: print " (should NOT occur)";
     2: print " (should NOT occur in full-line input)";
    }
  }
  else if (restr == 2) {
    switch (res) {
     -1: print " (should NOT occur)";
    }
  }
];

[ TestAccentsIn inbufvar ix ch len;

  print "This tests input of accented (actually, all)
    characters. Enter a line of text; it will be broken down,
    and you will see a list of what characters your interpreter
    thought you typed. Remember that the interpreter is supposed to
    reduce everything to lower-case.^^";
  print "Hit Enter by itself to return to the main menu.^";

  while (1) {
    print "^AccentedInput> ";
    inbuf->0 = (INBUFSIZE-3);
    inbuf->1 = 0;
    inbufvar = inbuf;
    ix = 0;
    @aread inbufvar ix;
    new_line;
    len = inbuf->1;
    if (len == 0)
      return;
    for (ix=0: ix<len: ix++) {
      ch = inbuf->(2+ix);
      print "code=", ch, ": ";
      DescChar(ch, 1);
      new_line;
    }
  }

];

[ TestAccentsCharIn ch;

  print "This tests input of accented (actually, all)
    characters. Type characters; you will see what characters your
    interpreter thought you typed. Hit ~.~ to return to the main
    menu.^^";

  while (1) {
    print "AccentedInput> ";
    ch = GetKey();
    if (ch == '.') {
      print "^^Test finished.^";
      return;
    }
    print "^code=", ch, ": ";
    DescChar(ch, 2);
    new_line;
  }
];

