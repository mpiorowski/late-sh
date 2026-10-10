
Array colortab -->
  "" "" "Black" "Red" "Green" "Yellow" "Blue" "Magenta" "Cyan" "White";

[ TestColor val fg bg claim;

  val = $0->1;

  if (val & 1) {
    print "Interpreter claims that colored text IS available.^";
    claim = 1;
  }
  else {
    print "Interpreter claims that colored text IS NOT available.^";
    claim = 0;
  }

  if (claim)
    print "^In the square below, you should see";
  else
    print "^If it was, in the square below, you would see";

  print " vertical stripes of background color (matching the column
    headers) and horizontal rows of foreground color (that is, all
    # marks in a row the same color.)^";

  font off;

  new_line;
  print "Bk Rd Gr Yl Bl Mg Cy Wh^";
  for (fg=2:fg<10:fg++) {
    for (bg=2:bg<10:bg++) {
      @set_colour fg bg;
      print "## ";
    }
    @set_colour 1 1;
    print ": ", (string)colortab-->fg, "^";
  }

  font on;

];
