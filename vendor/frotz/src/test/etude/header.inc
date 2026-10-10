
[ TestHeader val val2;

  val = $32->0;
  val2 = $32->1;
  if (val == 0 && val2 == 0) {
    print "Your interpreter does not claim to follow any particular version
      of the Z-Spec.^^";
  }
  else {
    print "Your interpreter claims to follow revision ", val, ".", val2,
      " of the Z-Spec.^^";
  }

  val = $0->1;

  if (val & 1) {
    print "Interpreter claims that colored text IS available.^";
  }
  else {
    print "Interpreter claims that colored text IS NOT available.^";
  }

  if (val & 4) {
    print "Interpreter claims that emphasized (bold) text IS
      available.^";
  }
  else {
    print "Interpreter claims that emphasized (bold) text IS NOT
      available.^";
  }

  if (val & 8) {
    print "Interpreter claims that italic (or underlined) text IS
      available.^";
  }
  else {
    print "Interpreter claims that italic (or underlined) text IS
      NOT available.^";
  }

  if (val & 16) {
    print "Interpreter claims that fixed-width text IS available.^";
  }
  else {
    print "Interpreter claims that fixed-width text IS NOT
      available.^";
  }

  if (val & 32) {
    print "Interpreter claims that sound effects ARE available.^";
  }
  else {
    print "Interpreter claims that sound effects ARE NOT available.^";
  }

  if (val & 128) {
    print "Interpreter claims that timed input IS available.^";
  }
  else {
    print "Interpreter claims that timed input IS NOT available.^";
  }

  new_line;
  val = $10->1;

  if (val & 16) {
    print "Interpreter claims that ~undo~ IS available.^";
  }
  else {
    print "Interpreter claims that ~undo~ IS NOT available.^";
  }
];
