$i = 0;
$a = function () use ($i) : int {
    return $i + 1;
};
$a();