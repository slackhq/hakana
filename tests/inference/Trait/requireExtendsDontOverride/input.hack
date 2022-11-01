class A {
  public function getStr(): ?string {
    return rand(0, 1) ? "a" : null;
  }
}

class B extends A {
  public function getStr(): string {
    return "a";
  }
}

trait T {
  require extends A;
}

class C extends B {
  use T;
  public function foo(): string {
    return $this->getStr();
  }
}