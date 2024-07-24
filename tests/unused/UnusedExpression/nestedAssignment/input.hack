function foo() : void {
  $a = 0;
  $b = 0;

  while (rand(0, 1)) {
    $a = $a + 1;
    $b = $b + 1;
  }

  echo $a;
}