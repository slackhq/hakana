function processFiles(): vec<string> {
    // Simulate the $ret pattern where a variable is assigned before loop,
    // used in loop, then reassigned multiple times inside loop
    $ret = vec['file1', 'file2', 'file3'];
    $processed = vec[];

    foreach ($ret as $file) {
        // This use of $ret comes from the assignment before the loop

        if (rand(0, 1)) {
            // First reassignment - should not error
            $ret = vec['downloaded'];
            bar($ret[0]);
        }

        if (rand(0, 1)) {
            // Second reassignment - should not error
            // Even though there's a use of $ret at the top (foreach),
            // that use comes from the pre-loop assignment, not from
            // the first inside reassignment
            $ret = vec['updated'];
            bar($ret[0]);
        }

        $processed[] = $file;
    }

    return $processed;
}

function bar(string $_s): void {}
