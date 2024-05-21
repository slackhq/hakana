trait T {
  public function f(): void {
    if ($this is A) { }
  }
}

final class A {
  use T;
}

final class B {
  use T;
}