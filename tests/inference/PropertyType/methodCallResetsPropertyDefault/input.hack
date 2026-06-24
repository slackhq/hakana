final class A
{
    public ?int $foo = null;

    public function setFoo(): void
    {
        $this->foo = 5;
    }
}

function bar(A $a): void {
    $a->foo = null;

    if (rand(0, 1) !== 0) {
        if (rand(0, 1) !== 0) {
            $a->setFoo();
        }
        
        if ($a->foo is nonnull) {}
    }
}
