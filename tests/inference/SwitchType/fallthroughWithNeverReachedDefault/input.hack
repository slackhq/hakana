enum Unit: int as int {
  A = 0;
  B = 1;
  C = 2;
  D = 3;
}

function my_func(Unit $x): void {
  switch ($x) {
    case Unit::A:
    case Unit::B:
    case Unit::C:
    case Unit::D:
        break;
    default:
    	echo "a";
    }
}