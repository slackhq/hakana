interface I {
    public function f(string $a, int $b): void;
}

class C implements I {
    public function f(string $a = "a", int $b = 1, float ...$rest): void {}
}

(new C())->f();
(new C())->f("b");
(new C())->f("b", 3);
(new C())->f("b", 3, 0.5);
(new C())->f("b", 3, 0.5, 0.8);