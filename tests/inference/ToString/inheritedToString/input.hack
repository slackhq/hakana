class A {
    public function __toString() {
        return "hello";
    }
}
class B {
    public function __toString() {
        return "goodbye";
    }
}
class C extends B {}

$c = new C();
echo (string) $c;