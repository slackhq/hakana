enum Foo: string {
  A = 'A';
  B = 'B';
}

function takesFoo(Foo $f) {
  switch ($f) {
    case Foo::A:
      $a;
    default:
      break;
  }
}