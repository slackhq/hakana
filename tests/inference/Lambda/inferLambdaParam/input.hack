function foo((function(int):int) $fn): void {
    echo $fn(5);
}

foo($i ==> $i);