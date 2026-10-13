
[ TestUndo claim ix cont result;

  print "This tests the interpreter's ability to do single and multiple ~undo~
    commands.^^";

  ! snarf bit 4 of the low (second) word of $10-11.
  ix = $10->1;
  if (ix & $10)
    claim = 1;
  else
    claim = 0;

  if (claim)
    print "Your interpreter claims (by its header bit) that it DOES support undo.^^";
  else
    print "Your interpreter claims (by its header bit) that it DOES NOT support undo.^^";

  cont = 1;
  result = 0;
  ! result codes: 0 for total failure of the undo
  ! facility. -1 if the interpreter claims (via save_undo returning
  ! -1) that it doesn't support undo. 1 if it supports single undo. 2
  ! if it supports multiple undo. -2 if the user aborted before
  ! testing.

  print "Simulating first move...^";
  @save_undo ix;
  switch (ix) {
   -1:
    print "Save failed -- interpreter claims that it does not
      support ~undo~.^";
    cont = 0;
    result = -1;
   0:
    print "Save failed.^";
    cont = 0;
    result = 0;
   1:
    print "Save succeeded.^";
   2:
    print "Undo succeeded (undid first move).^";
    cont = 0;
    result = 2;
   default:
    print "Save gave unknown result code ", ix, " -- ERROR.^";
    cont = 0;
    result = 0;
  }

  if (cont) {
    print "^Simulating second move...^";
    @save_undo ix;
    switch (ix) {
     -1:
      print "Save failed -- interpreter claims that it does not
	support ~undo~.^";
      cont = 0;
      result = -1;
     0:
      print "Save failed.^";
      cont = 0;
      result = 0;
     1:
      print "Save succeeded.^";
     2:
      print "Undo succeeded (undid second move).^";
      print "^Hit ~.~ to abort this test, or any other key to try a
	second ~undo~. (In many interpreters, executing the second
	~undo~ will return to exactly the same point as the first
	one. If this occurs, and you see this message a second time
	(or more), your interpreter only supports single-move
	undo.)^";
      print "MultipleUndo> ";
      ix = GetKey();
      new_line;
      if (ix == '.') {
	cont = 0;
	result = 1;
	break;
      }
      print "^Second undo...^";
      @restore_undo ix;
      switch (ix) {
       0:
	print "Undo failed.^";
       default:
	print "Undo gave unknown result code ", ix, " -- ERROR.^";
      }
      result = 0;
      cont = 0;
     default:
      print "Save gave unknown result code ", ix, " -- ERROR.^";
      cont = 0;
      result = 0;
    }
  }

  if (cont) {
    print "^Hit ~.~ to abort this test, or any other key to try
      ~undo~.^";
    print "SingleUndo> ";
    ix = GetKey();
    new_line;
    if (ix == '.') {
      cont = 0;
      result = -2;
    }
    else {
      print "^First undo...^";
      @restore_undo ix;
      switch (ix) {
       0:
	print "Undo failed.^";
	cont = 0;
	result = 0;
       default:
	print "Undo gave unknown result code ", ix, " -- ERROR.^";
	cont = 0;
	result = 0;
      }
    }
  }

  new_line;
  switch (result) {
   -2:
    print "Test cancelled.^";
   -1:
    if (claim)
      print "Your interpreter claims to support ~undo~, but the
	@@64save_undo opcode returned -1, indicating that the
	interpreter does not support ~undo~. ", (string)SectionBad;
    else
      print "Your interpreter claims to not support ~undo~, and the
	@@64save_undo opcode returned -1, corroborating this. ",
	(string)SectionOk;
   0:
    if (claim)
      print "Your interpreter claims to support ~undo~, but it
	didn't work. ", (string)SectionBad;
    else
      print "Your interpreter claims to not support ~undo~, and in
	fact ~undo~ failed. However, it did not return -1 from the
	@@64save_undo opcode, which it should. ", (string)SectionBad;
   1:
    if (claim)
      print "Your interpreter claims to support ~undo~, and it
	does. ", (string)SectionOk;
    else
      print "Your interpreter claims to not support ~undo~, but in
	fact it does anyway. ", (string)SectionBad;
   2:
    if (claim)
      print "Your interpreter claims to support ~undo~, and it does.
	In fact, it supports multiple ~undo~. ", (string)SectionOk;
    else
      print "Your interpreter claims to not support ~undo~, but in
	fact it does anyway. In fact, it supports multiple ~undo~. ",
	(string)SectionBad;
  }
];

