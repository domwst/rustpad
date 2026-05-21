import { Box, Button, HStack, Select, Text } from "@chakra-ui/react";
import { type ChangeEvent, useMemo } from "react";

type ReplayControlsProps = {
  isPlaying: boolean;
  positionMs: number;
  durationMs: number;
  speed: number;
  activityBuckets: number[];
  darkMode: boolean;
  onPlayPause: () => void;
  onPositionChange: (positionMs: number) => void;
  onSpeedChange: (speed: number) => void;
};

function ReplayControls({
  isPlaying,
  positionMs,
  durationMs,
  speed,
  activityBuckets,
  darkMode,
  onPlayPause,
  onPositionChange,
  onSpeedChange,
}: ReplayControlsProps) {
  const maxActivity = useMemo(
    () => Math.max(1, ...activityBuckets),
    [activityBuckets],
  );
  const progress = durationMs > 0 ? (positionMs / durationMs) * 100 : 0;

  return (
    <Box borderTopWidth={1} borderColor="gray.600" p={2} flexShrink={0}>
      <HStack spacing={3}>
        <Button size="sm" onClick={onPlayPause}>
          {isPlaying ? "Pause" : "Play"}
        </Button>
        <Text minW="88px" fontSize="sm" color="gray.400">
          {formatTime(positionMs)} / {formatTime(durationMs)}
        </Text>
        <Box
          flex={1}
          h="24px"
          position="relative"
          borderWidth={1}
          borderColor={darkMode ? "gray.500" : "gray.300"}
          bgColor={darkMode ? "gray.700" : "white"}
          overflow="hidden"
          cursor="pointer"
        >
          <HStack h="100%" spacing={0} align="stretch">
            {activityBuckets.map((count, index) => {
              const intensity = count / maxActivity;
              return (
                <Box
                  key={index}
                  flex={1}
                  bgColor={
                    count === 0
                      ? darkMode
                        ? "rgba(255,255,255,0.05)"
                        : "rgba(0,0,0,0.03)"
                      : darkMode
                        ? `rgba(99, 179, 237, ${0.2 + intensity * 0.8})`
                        : `rgba(49, 130, 206, ${0.12 + intensity * 0.55})`
                  }
                />
              );
            })}
          </HStack>
          <Box
            position="absolute"
            top={0}
            bottom={0}
            left={`${Math.min(100, Math.max(0, progress))}%`}
            borderLeft={darkMode ? "2px solid white" : "2px solid #1a202c"}
            boxShadow={
              darkMode
                ? "0 0 0 1px rgba(0,0,0,0.35)"
                : "0 0 0 1px rgba(255,255,255,0.65)"
            }
            pointerEvents="none"
          />
          <Box
            as="input"
            type="range"
            aria-label="Replay position"
            min={0}
            max={Math.max(durationMs, 1)}
            value={Math.min(positionMs, durationMs)}
            onChange={(event: ChangeEvent<HTMLInputElement>) =>
              onPositionChange(Number(event.target.value))
            }
            position="absolute"
            inset={0}
            w="100%"
            h="100%"
            opacity={0}
            cursor="pointer"
          />
        </Box>
        <Select
          size="sm"
          w="86px"
          value={speed}
          onChange={(event) => onSpeedChange(Number(event.target.value))}
        >
          {[0.5, 1, 2, 4, 8].map((value) => (
            <option key={value} value={value}>
              {value}x
            </option>
          ))}
        </Select>
      </HStack>
    </Box>
  );
}

function formatTime(ms: number) {
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

export default ReplayControls;
