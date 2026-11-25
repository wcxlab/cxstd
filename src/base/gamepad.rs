/// Button indices in the Web Standard Gamepad mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum GamepadButtonCode {
    South = 0,
    East = 1,
    West = 2,
    North = 3,
    LeftShoulder = 4,
    RightShoulder = 5,
    LeftTrigger = 6,
    RightTrigger = 7,
    CenterLeft = 8,
    CenterRight = 9,
    LeftStick = 10,
    RightStick = 11,
    DPadUp = 12,
    DPadDown = 13,
    DPadLeft = 14,
    DPadRight = 15,
    Home = 16,
}

/// Analog stick axis indices in the Web Standard Gamepad mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum GamepadAxis {
    LeftStickX = 0,
    LeftStickY = 1,
    RightStickX = 2,
    RightStickY = 3,
}
