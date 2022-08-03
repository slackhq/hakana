namespace Bat;

interface I  {
  public function foo();
  public function bar();
}
abstract class A implements I {
  public function foo() {
    return "hello";
  }
}
class B extends A {
  public function bar() {
    return "goodbye";
  }
}