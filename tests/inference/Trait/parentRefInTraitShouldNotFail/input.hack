trait T {
  public function foo(): void {
    parent::foo();
  }
}
class A {
  public function foo(): void {}
}
class B extends A {
  use T;
}