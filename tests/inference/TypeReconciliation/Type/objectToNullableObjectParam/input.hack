class A {}

class B {
    public function barBar(A $a = null) : void {}
}

$b = new B();
$b->barBar(new A());