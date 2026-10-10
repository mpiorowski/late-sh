
[ TestTimedString claim inbufvar ix;

  ix = $0->1;
  if (ix & 128) {
    claim = 1;
    print "Your interpreter claims (by its header bit) that it DOES
      support timed input.^^";
  }
  else {
    claim = 0;
    print "Your interpreter claims (by its header bit) that it DOES
      NOT support timed input.^^";
  }

  print "When you begin this test, you should be able to enter a
    line of input in the usual fashion. Every three seconds, a line
    of text will appear. (If the lines appear at the rate of one
    every thirty seconds, your interpreter is using the incorrect
    timing rate caused by an old bug in ZIP.)^^";
  print "Interrupts actually are occurring once per second; every
    third interrupt prints a line of text, and the other two have no
    visible effect. After each line, your input should be redrawn so
    that you can continue typing and editing it.^^";

  print "Hit ~.~ to return to the main menu, or any other key to begin
    the test. Type ~.~ on a line by itself to stop the test.^^";

  print "TimedString> ";
  ix = GetKey();
  if (ix == '.')
    return;
  new_line;

  print "Beginning test...^^";

  timedcharflag = -93;
  timedcharcounter = 0;

  while (1) {
    print "TimedString> ";
    inbuf->0 = (INBUFSIZE-3);
    inbuf->1 = 0;
    inbufvar = inbuf;
    ix = 0;
    @aread inbufvar 0 10 TimedStringSplot ix;
    new_line;
    if (inbuf->1 == 0)
      print "You just typed a blank line. (Type ~.~ to stop this
	test.)^^";
    else {
      print "You just typed ~";
      for (ix=0: ix<inbuf->1: ix++)
      	print (char)(inbuf->(2+ix));
      print "~.^^";
    }
    if (inbuf->1 == 1 && inbuf->2 == '.')
      break;
  }
  print "Test terminated.^";

  if (timedcharflag == -93) {
    print "The timing interrupt function was not called at
      all. Either your interpreter does not support timed input, or
      you terminated the test before one second elapsed. (Or your
      interpreter has the ~slow~ bug and you terminated the test
      before ten seconds elapsed.)^^";
    if (claim == 0)
      print "Your interpreter claims to not support timed
	input. ", (string)SectionOk;
    else
      print "Your interpreter claims to support timed input. ",
	(string)SectionBad;
  }
  else {
    if (timedcharflag == 0) {
      print "Your interpreter calls the timing interrupt function
	with no arguments. ";
      if (claim == 0)
	print "This is correct, except that your interpreter claims
	  not to support timed input at all. ", (string)SectionBad;
      else
	print (string)SectionOk;
    }
    else {
      print "Your interpreter calls the timing interrupt function
	with an argument. ", (string)SectionBad;
    }
  }

];

[ TimedStringSplot arg;
  timedcharflag = arg;
  timedcharcounter++;
  if (timedcharcounter == 3) {
    timedcharcounter = 0;
    print "^[Every three seconds....]^TimedString> ";
  }
  return 0;
];
