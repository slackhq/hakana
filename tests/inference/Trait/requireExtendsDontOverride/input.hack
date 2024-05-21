abstract class A {
  public function getStr(): ?string {
    return rand(0, 1) ? "a" : null;
  }
}

abstract class B extends A {
  public function getStr(): string {
    return "a";
  }
}

trait T {
  require extends A;
}

final class C extends B {
  use T;
  public function foo(): string {
    return $this->getStr();
  }
}