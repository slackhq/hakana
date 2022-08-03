trait T {
  public function f(): void {
    if ($this is A) { }
  }
}

class A {
  use T;
}

class B {
  use T;
}