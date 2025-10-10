namespace Bat;

interface I  {
  public function foo();
  public function bar();
}
abstract class A implements I {
  <<__Override>>
  public function foo() {
    return "hello";
  }
}
final class B extends A {
  <<__Override>>
  public function bar() {
    return "goodbye";
  }
}