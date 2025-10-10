abstract class A {
  public function getStr(): ?string {
    return rand(0, 1) ? "a" : null;
  }
}

abstract class B extends A {
  <<__Override>>
  public function getStr(): string {
    return "a";
  }
}

trait T1 {
  require extends B;

  public function foo(): string {
    return $this->getStr();
  }
}
