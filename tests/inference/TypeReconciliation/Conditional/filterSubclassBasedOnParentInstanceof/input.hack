abstract class A {}
final class B extends A {
   public function foo() : void {}
}

abstract class C {}
final class D extends C {}

$b_or_d = rand(0, 1) ? new B() : new D();

if ($b_or_d is A) {
    $b_or_d->foo();
}