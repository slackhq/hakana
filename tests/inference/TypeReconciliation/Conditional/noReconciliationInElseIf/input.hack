class A {}
$a = rand(0, 1) ? new A() : null;

if (rand(0, 1)) {
    // do nothing
} else if (!$a) {
    $a = new A();
}

if ($a) {}