interface I1 {}

final class A1 implements I1{}

final class B1 implements I1 {}

final class C1 {
    public vec<I1> $is = dict[];
}

$c = new C1();
$c->is = vec[new A1()];
$c->is = vec[new A1(), new A1()];
$c->is = vec[new A1(), new B1()];