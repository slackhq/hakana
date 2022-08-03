function foo(string $a, string $b) : int {
    $aTime = strtotime($a);
    $bTime = strtotime($b);

    return $aTime - $bTime;
}