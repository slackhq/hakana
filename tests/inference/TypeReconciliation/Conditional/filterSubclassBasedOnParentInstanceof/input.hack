class A {}
class B extends A {
   public function foo() : void {}
}

class C {}
class D extends C {}

$b_or_d = rand(0, 1) ? new B : new D;

if ($b_or_d is A) {
    $b_or_d->foo();
}