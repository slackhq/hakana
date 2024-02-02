function foo(): dict<string, int> {
    $log = new HH\Lib\Ref(dict[]);

    $log_op = (string $s, int $i) ==> {
        $log_vector = $log->get();
        $log_vector[$s] = $i;
        $log->set($log_vector);
    };

    $log_op('a', 1);
    $log_op('b', 2);
    
    return $log->get();
}