
[ TestExitText ix;

  print "This tests if you can read text which is displayed
    immediately before the program quits. (It is not clear whether
    this is covered by the Z-Spec. However, some games -- including some
    original Infocom games -- display important closing text and then
    immediately quit.)^^";
  print "Hit ~.~ to return to the main menu, or any other key to print a
    line of text and then quit. If the line of text does not appear,
    or appears and vanishes too quickly to read, your interpreter will
    have trouble with those games.^^";

  print "ClosingText> ";
  ix = GetKey();
  if (ix == '.') {
    print "^Test aborted.^";
    return;
  }

  print "^^This is a final line of text. Goodbye.^";
  quit;

];
