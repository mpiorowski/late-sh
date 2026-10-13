
[ TestGivenInput ix inbufvar;

  print "This tests whether your interpreter supports being handed
    input by the game file. (For example, after a function key is hit
    in Beyond Zork, the command prompt appears with text already in
    place -- whatever text you had typed before you interrupted it
    with the function key.)^^";

  print "The command prompt below should appear with the word
    ~Given~ already visible after it. You should be able to type
    more characters, or go back and edit the given character as if
    you had typed them yourself.^^";

  print "If you see the word ~Given~ twice, your interpreter is
    printing the given characters, which it shouldn't (that's
    the responsibility of the game file.)^^";

  print "Preload> Given";
  inbuf->0 = (INBUFSIZE-3);
  inbuf->1 = 5;

  inbuf->2 = 'G';
  inbuf->3 = 'i';
  inbuf->4 = 'v';
  inbuf->5 = 'e';
  inbuf->6 = 'n';

  inbufvar = inbuf;
  ix = 0;
  @aread inbufvar ix;

    if (inbuf->1 == 0)
      print "You just typed a blank line.^";
    else {
      print "You just typed ~";
      for (ix=0: ix<inbuf->1: ix++)
      	print (char)(inbuf->(2+ix));
      print "~.^";
    }

];
