trait T {
    public static function filterInstance(mixed $instance): ?this {
        return $instance is this ? $instance : null;
    }
}

class A {
    use T;
}