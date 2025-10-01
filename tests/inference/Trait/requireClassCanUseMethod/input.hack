abstract class A {
  public function getStr(): ?string {
    return rand(0, 1) ? "a" : null;
  }
}

trait T1 {
  require class B;

  public function foo(): string {
    return $this->getStr();
  }
}

final class B extends A {
  use T1;

  public function getStr(): string {
    return "a";
  }
}