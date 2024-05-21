final class A {}

if (!class_exists(A::class)) {
    new \RuntimeException();
}