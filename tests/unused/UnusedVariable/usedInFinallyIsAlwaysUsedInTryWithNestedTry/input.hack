$step = 0;
try {
    try {
        $step = 1;
    } finally {
    }
    $step = 2;
    $step = 3;
} finally {
    echo $step;
}
                