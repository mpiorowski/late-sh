
[ TestDivLine str answer correct;
  print (string) str, " = ", answer;
  if (answer == correct) {
    print " (ok)^";
  }
  else {
    print " (ERROR, should be ", correct, "!)^";
    errflag = 1;
  }
];

[ TestDiv x y z;
  print "This tests signed multiplication, division, and modulo
    operations. All these operations are supposed to be
    signed. (The Z-Spec 0.2 erroneously says they are unsigned; this is
    corrected in 0.99.)^^";

  print "I am assuming the convention that division always rounds towards
    zero (not towards negative infinity), and (A % B) always has the
    same sign as A. These conventions seem to be common among existing
    C/C++ compilers. The Infocom interpreters also follow these conventions.
    (But they are not guaranteed by the C and C++ standards. Those only
    require that (A/B)*B + (A%B) == A, for all A and all nonzero B.)^^";

  errflag = 0;

  x = 13; y = 5; z = x * y;
  TestDivLine("13 * 5", z, 65);
  x = 13; y = -5; z = x * y;
  TestDivLine("13 * -5", z, -65);
  x = -13; y = 5; z = x * y;
  TestDivLine("-13 * 5", z, -65);
  x = -13; y = -5; z = x * y;
  TestDivLine("-13 * -5", z, 65);

  x = 13; y = 5; z = x / y;
  TestDivLine("13 / 5", z, 2);
  x = 13; y = -5; z = x / y;
  TestDivLine("13 / -5", z, -2);
  x = -13; y = 5; z = x / y;
  TestDivLine("-13 / 5", z, -2);
  x = -13; y = -5; z = x / y;
  TestDivLine("-13 / -5", z, 2);

  x = 13; y = 5; z = x % y;
  TestDivLine("13 % 5", z, 3);
  x = 13; y = -5; z = x % y;
  TestDivLine("13 % -5", z, 3);
  x = -13; y = 5; z = x % y;
  TestDivLine("-13 % 5", z, -3);
  x = -13; y = -5; z = x % y;
  TestDivLine("-13 % -5", z, -3);

  if (errflag == 0) {
    print "^", (string)SectionOk;
  }
  else {
    print "^", (string)SectionBad;
  }
];
