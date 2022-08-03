function test(): int {
  $x = 0;
  $y = 1;
  $z = 2;
  for ($i = 0; $i < 3; $i++) {
    $x = $y;
    $y = $z;
    $z = 5;
  }
  return $x;
}