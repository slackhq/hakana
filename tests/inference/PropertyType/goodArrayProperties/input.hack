interface I1 {}

class A1 implements I1{}

class B1 implements I1 {}

class C1 {
    public vec<I1> $is = dict[];
}

$c = new C1;
$c->is = vec[new A1];
$c->is = vec[new A1, new A1];
$c->is = vec[new A1, new B1];