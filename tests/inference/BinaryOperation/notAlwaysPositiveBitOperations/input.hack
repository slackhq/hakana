
$a = 1;
$b = 1;
$c = 32;
$d = 64;
$e = 2;

if (0 === ($a ^ $b)) {
    echo "Actually, zero\n";
}

if (0 === ($a & $e)) {
    echo "Actually, zero\n";
}

if (0 === ($a >> $b)) {
    echo "Actually, zero\n";
}

if (8 === PHP_INT_SIZE) {
    if (0 === ($a << $d)) {
        echo "Actually, zero\n";
    }
}