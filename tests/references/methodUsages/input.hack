class Calculator {
    public function add(int $a, int $b): int {
        return $a + $b;
    }

    public function multiply(int $a, int $b): int {
        return $a * $b;
    }
}

function test(): void {
    $calc = new Calculator();
    $sum = $calc->add(1, 2);
    $product = $calc->multiply(3, 4);
    $combined = $calc->add($calc->multiply(2, 3), 1);
}
