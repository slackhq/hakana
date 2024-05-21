final class MyException extends Exception {}

function foo(): void {
    try {
      bar();
      $a = null;
    } catch (Exception $e) {
      $a = 2;
      return;
    } finally {
      if ($a is null) {}
    }
}

function bar(): void {}