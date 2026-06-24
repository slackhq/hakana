abstract class A {
  public function getStr(): ?string {
    return rand(0, 1) !== 0 ? "a" : null;
  }
}

trait T2 {
  public function helper(): void {}
}

trait T1 {
  require extends B;
  use T2;

  public function foo(): string {
    $this->helper();
    self::staticMethodOnB();
    return $this->getStr();
  }
}

abstract class B extends A {
  <<__Override>>
  public function getStr(): string {
    return "a";
  }

  public static function staticMethodOnB(): void {}
}
