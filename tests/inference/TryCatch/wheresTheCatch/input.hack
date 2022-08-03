function foo() : bool {
    try {
        return true;
    } finally {
    }
}

function bar() : bool {
    try {
        // do nothing
    } finally {
        return true;
    }
}