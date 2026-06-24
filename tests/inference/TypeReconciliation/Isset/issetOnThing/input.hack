function foo() : void {
    $p = vec[false, false];
    $i = rand(0, 1);
    if (rand(0, 1) !== 0 && isset($p[$i])) {
        $p[$i] = true;
    }

    foreach ($p as $q) {
        if ($q) {}
    }
}