enum Unit: int as int {
  A = 0;
  B = 1;
  C = 2;
  D = 3;
}

function my_func(Unit $x): string {
  $foo = '';
  switch($x) {
    case Unit::A:
    	$foo.= 'a';
        // FALLTHROUGH
    case Unit::B:
        $foo .= 'b';
        // FALLTHROUGH
    case Unit::C:
        $foo .= 'c';
        // FALLTHROUGH
    case Unit::D:
    	$foo .= 'd';
        break;
    }
    return $foo;
}

function my_func2(Unit $x): string {
  $foo = '';

  if ($x == Unit::A || $x == Unit::B || $x == Unit::C) {
    if ($x == Unit::A || $x == Unit::B) {
      if ($x == Unit::A) {
        $foo .= 'a';
      }

      $foo .= 'b';
    }

    $foo .= 'c';
  }

  $foo .= 'd';
  break;
}