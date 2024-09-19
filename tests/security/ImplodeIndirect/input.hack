$unsafe = HH\global_get('_GET')['unsafe'] as dict<_, _>;
echo implode(" ", $unsafe);