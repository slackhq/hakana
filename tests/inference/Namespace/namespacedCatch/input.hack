namespace Foo;

use type Exception;

function bar() {
    try {
        \rand(0, 1);
    } catch (Exception $e) {
        echo $e->getMessage();
    }
}