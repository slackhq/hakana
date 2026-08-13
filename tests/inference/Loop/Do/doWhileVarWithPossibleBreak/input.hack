$a = false;

do {
    if (rand(0, 1) !== 0) {
        break;
    }
    if (rand(0, 1) !== 0) {
        $a = true;
        break;
    }
    $a = true;
}
while (rand(0,100) === 10);
