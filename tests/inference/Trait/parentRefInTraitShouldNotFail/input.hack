trait T {
  public function foo(): void {
    parent::foo();
  }
}
abstract class A {
  public function foo(): void {}
}
final class B extends A {
  use T;
}