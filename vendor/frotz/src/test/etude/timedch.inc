
Global timedcharflag;
Global timedcharcounter;

[ TestTimedChar claim ix;

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

  print "When you begin this test, asterisks should appear at the rate
    of one per second. If they appear at the rate of one every ten
    seconds, your interpreter is using the incorrect timing rate
    caused by an old bug in ZIP. If an entire line of ten asterisks
    appears all at once every ten seconds, your interpreter is not
    properly flushing its buffer.^^";

  while (1) {
    print "Hit ~.~ to return to the main menu, or any other key to begin
      the test. Hit any key to stop the test while it is running.^^";

    print "TimedKey> ";
    ix = GetKey();
    if (ix == '.')
      return;
    new_line;

    timedcharcounter = 0;
    timedcharflag = -93;
    @read_char 1 10 TimedCharSplot ix;
    new_line;

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
    new_line;
  }
];

[ TimedCharSplot arg;
  timedcharflag = arg;
  print "* ";
  timedcharcounter++;
  if (timedcharcounter == 10) {
    new_line;
    timedcharcounter = 0;
  }
  return 0;
];
