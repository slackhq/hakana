$arr = vec[2, 3, 4, 5];

$direct_closure_result = array_reduce(
    $arr,
    (int $carry, int $item) ==> {
        return $_GET["boo"];
    },
    1
);