class A {
    public function aa(): ?string {
        return "bar";
    }
}

class B extends A {
    public static function aa(): ?string {
        return rand(0, 1) ? "bar" : null;
    }
}

class C extends A {
    public static function aa(): ?string {
        return "bar";
    }
}