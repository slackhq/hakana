function foo(): vec<int> {
    $log = new HH\Lib\Ref(vec[]);

    $log_op = (int $i) ==> {
        $log_vector = $log->get();
        $log_vector[] = $i;
        $log->set($log_vector);
    };

    $log_op(1);
    $log_op(2);
    
    return $log->get();
}