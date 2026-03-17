abstract class A {
  public function getStr(): ?string {
    return rand(0, 1) ? "a" : null;
  }
}

trait T2 {
  public function helper(): void {}
}

trait T1 {
  require class B;
  use T2;

  public function foo(): string {
    $this->helper();
    return $this->getStr();
  }
}

final class B extends A {
  use T1;

  <<__Override>>
  public function getStr(): string {
    return "a";
  }
}

function foo(): void {
  $b = new B();
  $b->foo();
  $b->getStr();
}
