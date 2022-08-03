$a = (bool) rand(0, 1);

if (rand(0, 1)) {
    $a = null;
}

if ($a !== (bool) rand(0, 1)) {
    echo $a === false ? "a" : "b";
}