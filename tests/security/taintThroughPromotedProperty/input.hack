final class User {
    public function __construct(public string $userId) {}
}

function bar(): User {
    return new User(HH\global_get('_GET')["user_id"]);
}

function calls_bar() {
    $bar = bar();
    echo $bar->userId;
}