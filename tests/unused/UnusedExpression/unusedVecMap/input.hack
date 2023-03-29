function foo(): void {
    Vec\map(vec[1, 2, 3, 3], ($i) ==> $i);
}

function bar(): void {
    Vec\map(vec[1, 2, 3, 3], ($i) ==> {
        echo $i;
        return $i;
    });
}
