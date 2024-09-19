final class A {
    public function getUserId() : string {
        return (string) HH\global_get('_GET')['user_id'];
    }

    public function getAppendedUserId() : string {
        return 'aaaa' . $this->getUserId();
    }

    public function deleteUser(MySqlHandler $conn) : void {
        $userId = $this->getAppendedUserId();
        $conn->exec('delete from users where user_id = ' . $userId);
    }
}

final class MySqlHandler {
    public function exec(
        <<\Hakana\SecurityAnalysis\Sink('Sql')>> string $sql
    ) : void {}
}