class A
{
    public ?int $foo = null;

    public function setFoo(): void
    {
        $this->foo = 5;
    }
}

function bar(A $a): void {
    $a->foo = null;

    while (rand(0, 1)) {
        if (rand(0, 1)) {
            $a->setFoo();
        } else {
            if ($a->foo !== null) {
                break;
            }
        }
    }

    if ($a->foo !== null) {}
}