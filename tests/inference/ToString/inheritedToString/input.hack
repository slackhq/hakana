final class A {
    public function __toString() {
        return "hello";
    }
}
abstract class B {
    public function __toString() {
        return "goodbye";
    }
}
final class C extends B {}

$c = new C();
echo (string) $c;