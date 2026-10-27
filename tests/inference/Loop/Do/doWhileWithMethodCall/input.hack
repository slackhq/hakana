final class A {
    public function getParent(): ?A {
        return rand(0, 1) !== 0 ? new A() : null;
    }
}

$a = new A();

do {
    $a = $a->getParent();
} while ($a is nonnull);

if ($a is null) {}
