
[ TestStyles val;

  val = $0->1;

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

  new_line;
  style bold;
  print "Test of emphasized (bold) text.";
  style roman;
  print "^...normal font...^";

  style underline;
  print "Test of italic (or underlined) text.";
  style roman;
  print "^...normal font...^";

  style reverse;
  print "Test of reverse-video text.";
  style roman;
  print "^...normal font...^";

  font off;
  print "Test of fixed-width text (using ~font off~, which sets a
    header bit).";
  font on;
  print "^...normal font...^";

  @set_text_style 8;
  print "Test of fixed-width text (using ~@@64set_text_style 8~).";
  @set_text_style 0;
  new_line;

  print "^Now we will try combining styles. The Z-Spec does not
    require interpreters to display combined styles, so the lines
    below may not have all the styles they say they do.^^";

  style bold; style underline;
  print "Test of bold, italic text.";
  style roman;
  new_line;

  style bold; style reverse;
  print "Test of bold, reverse text.";
  style roman;
  new_line;

  style underline; style reverse;
  print "Test of italic, reverse text.";
  style roman;
  new_line;

  style bold; style underline; style reverse;
  print "Test of bold, italic, reverse text.";
  style roman;
  new_line;

  print "^The following are all printed with ~font off~ (that is, the
    header bit remains set, even though ~style roman~ statements
    occur.)^";
  font off;

  style bold;
  print "Test of fixed, bold text.";
  style roman;
  new_line;

  style underline;
  print "Test of fixed, italic text.";
  style roman;
  new_line;

  style reverse;
  print "Test of fixed, reverse text.";
  style roman;
  new_line;

  style bold; style underline;
  print "Test of fixed, bold, italic text.";
  style roman;
  new_line;

  style bold; style reverse;
  print "Test of fixed, bold, reverse text.";
  style roman;
  new_line;

  style underline; style reverse;
  print "Test of fixed, italic, reverse text.";
  style roman;
  new_line;

  style bold; style underline; style reverse;
  print "Test of fixed, bold, italic, reverse text.";
  style roman;
  new_line;

  font on;

  print "^End of styles test.^";
];
