
! This table is in order and complete from code 155 to 223 inclusive.

Array AccentList table [;
  "a-umlaut:@:a";
  "o-umlaut:@:o";
  "u-umlaut:@:u";
  "A-umlaut:@:A";
  "O-umlaut:@:O";
  "U-umlaut:@:U";
  "sz-ligature:@ss";
  ">>-quotes:@@162";
  "<<-quotes:@@163";
  "e-umlaut:@:e";
  "i-umlaut:@:i";
  "y-umlaut:@:y";
  "E-umlaut:@:E";
  "I-umlaut:@:I";
  "a-acute:@'a";
  "e-acute:@'e";
  "i-acute:@'i";
  "o-acute:@'o";
  "u-acute:@'u";
  "y-acute:@'y";
  "A-acute:@'A";
  "E-acute:@'E";
  "I-acute:@'I";
  "O-acute:@'O";
  "U-acute:@'U";
  "Y-acute:@'Y";
  "a-grave:@`a";
  "e-grave:@`e";
  "i-grave:@`i";
  "o-grave:@`o";
  "u-grave:@`u";
  "A-grave:@`A";
  "E-grave:@`E";
  "I-grave:@`I";
  "O-grave:@`O";
  "U-grave:@`U";
  "a-circumflex:@^a";
  "e-circumflex:@^e";
  "i-circumflex:@^i";
  "o-circumflex:@^o";
  "u-circumflex:@^u";
  "A-circumflex:@^A";
  "E-circumflex:@^E";
  "I-circumflex:@^I";
  "O-circumflex:@^O";
  "U-circumflex:@^U";
  "a-ring:@oa";
  "A-ring:@oA";
  "o-slash:@/o";
  "O-slash:@/O";
  "a-tilde:@~a";
  "n-tilde:@~n";
  "o-tilde:@~o";
  "A-tilde:@~A";
  "N-tilde:@~N";
  "O-tilde:@~O";
  "ae-ligature:@ae";
  "AE-ligature:@AE";
  "c-cedilla:@cc";
  "C-cedilla:@cC";
  "thorn:@th";
  "eth:@et";
  "Thorn:@Th";
  "Eth:@Et";
  "pound-symbol:@LL";
  "oe-ligature:@oe";
  "OE-ligature:@OE";
  "inverse-!:@!!";
  "inverse-?:@??";

];

[ TestAccents ix opt;

  print "This displays all the accented characters (encoding values
    155 to 223). You will have to inspect your interpreter's display
    to make sure they appear correctly.^^";
  print "For the record, an umlaut is two dots; an acute accent is
    the one that slants up to the right; a grave accent is the one
    that slants down to the right; a circumflex is a pointy hat; a
    tilde is a squiggly hat; a ring is a ring; a cedilla is the
    little hook that hangs down below the C. Thorn looks like a
    capital D whose vertical bar extends both up and down, and Eth
    looks like a D with a little cross-stroke.^^";

  print "NOTE: Inform 6.11 contradicts the Z-Spec 0.99 document, on
    the subject of the European angle-quotes (the ones that look
    like '>>' and '<<'). The Z-Spec says that the character '>>' is
    code 162, and '<<' is 163. However, Inform 6.11 (following the
    Z-Spec 0.2) compiles
    '@@64>>' as 163, and '@@64<<' as 162. The consensus is that the
    Z-Spec 0.2 and Inform 6.11 are wrong, and Z-Spec 0.99 and later
    are correct.^^";

  !ix = '@>>';
  !print "(This version of TerpEtude was compiled with Inform ";
  !inversion;
  !print ", which compiles '@@64>>' as ", ix, ", which your interpreter
  !  displays as '", (char)ix, "'. Got it?)^^";

  opt = 0;

  while (1) {

    if (opt >= 0) {
      if (opt & 4)
      	font off;
      if (opt & 2)
	style underline;
      if (opt & 1)
	style bold;
      for (ix=1: ix <= AccentList-->0: ix++) {
      	print (string) AccentList-->ix;
      	if (ix % 4 == 0)
      	  new_line;
      	else
      	  print "   ";
      }
      if (opt & 4)
      	font on;
      if (opt & 3)
	style roman;
      new_line; new_line;
    }

    print "Type a digit (0..7) to repeat this list in a different text
      style, or ~.~ to end this test.^";
    print "Options: 0: normal; 1: bold; 2: italic; 3: bold italic; 4:
      fixed-width; 5: fixed bold; 6: fixed italic; 7: fixed bold italic.^";
    print "^Accents> ";
    @read_char 1 ix;
    new_line;

    if (ix == '.')
      return;
    new_line;

    if (ix >= '0' && ix <= '7')
      opt = ix - '0';
    else
      opt = -1;
  }

];
