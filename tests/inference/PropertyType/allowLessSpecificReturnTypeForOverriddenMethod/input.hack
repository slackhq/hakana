abstract class A {
    public function aa(): ?string {
        return "bar";
    }
}

final class B extends A {
    public static function aa(): ?string {
        return rand(0, 1) ? "bar" : null;
    }
}

final class C extends A {
    public static function aa(): ?string {
        return "bar";
    }
}