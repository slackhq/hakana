$a = rand(0, 1) === 1;

if (rand(0, 1) !== 0) {
    $a = null;
}

if ($a !== (rand(0, 1) === 1)) {
    echo $a === false ? "a" : "b";
}
