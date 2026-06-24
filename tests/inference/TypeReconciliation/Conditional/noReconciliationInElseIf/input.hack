final class A {}
$a = rand(0, 1) !== 0 ? new A() : null;

if (rand(0, 1) !== 0) {
    // do nothing
} else if (!$a) {
    $a = new A();
}

if ($a) {}
