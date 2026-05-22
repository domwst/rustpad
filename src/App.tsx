import { Box, Flex, HStack, Icon, Text, useToast } from "@chakra-ui/react";
import Editor from "@monaco-editor/react";
import { editor } from "monaco-editor/esm/vs/editor/editor.api";
import type { VimAdapterInstance } from "monaco-vim";
import { useEffect, useRef, useState } from "react";
import { VscChevronRight, VscFolderOpened, VscGist } from "react-icons/vsc";
import { OpSeq } from "rustpad-wasm";
import useLocalStorageState from "use-local-storage-state";

import rustpadRaw from "../rustpad-server/src/rustpad.rs?raw";
import Footer from "./Footer";
import NameGate from "./NameGate";
import ReadCodeConfirm from "./ReadCodeConfirm";
import ReplayControls from "./ReplayControls";
import Sidebar from "./Sidebar";
import languages from "./languages.json";
import Rustpad, { CursorData, UserInfo } from "./rustpad";
import useHash from "./useHash";

const CURSOR_LABEL_VISIBLE_MS = 1500;

function getWsUri(id: string) {
  let url = new URL(`api/socket/${id}`, window.location.href);
  url.protocol = url.protocol == "https:" ? "wss:" : "ws:";
  return url.href;
}

function getReplayUri(id: string) {
  return new URL(`api/replay/${id}`, window.location.href).href;
}

function generateHue() {
  return Math.floor(Math.random() * 360);
}

function generateToken() {
  const bytes = new Uint8Array(24);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join(
    "",
  );
}

function getHostToken(id: string) {
  const key = `hostToken:${id}`;
  let token = localStorage.getItem(key);
  if (!token) {
    token = generateToken();
    localStorage.setItem(key, token);
  }
  return token;
}

function App() {
  const toast = useToast();
  const [language, setLanguage] = useState("plaintext");
  const [connection, setConnection] = useState<
    "connected" | "disconnected" | "desynchronized" | "closed"
  >("disconnected");
  const [users, setUsers] = useState<Record<number, UserInfo>>({});
  const [name, setName] = useLocalStorageState("name", {
    defaultValue: "",
  });
  const [hue, setHue] = useLocalStorageState("hue", {
    defaultValue: generateHue,
  });
  const [editor, setEditor] = useState<editor.IStandaloneCodeEditor>();
  const [darkMode, setDarkMode] = useLocalStorageState("darkMode", {
    defaultValue: false,
  });
  const [vimMode, setVimMode] = useLocalStorageState("vimMode", {
    defaultValue: false,
  });
  const rustpad = useRef<Rustpad>();
  const vimModeRef = useRef<VimAdapterInstance>();
  const vimStatusRef = useRef<HTMLDivElement>(null);
  const id = useHash();
  const [hostToken, setHostToken] = useState(() => getHostToken(id));
  const [isHost, setIsHost] = useState(false);
  const [closedAt, setClosedAt] = useState<number | null>(null);
  const [replayData, setReplayData] = useState<ReplayResponse>();
  const [replayPositionMs, setReplayPositionMs] = useState(0);
  const [replaySpeed, setReplaySpeed] = useState(1);
  const [replayPlaying, setReplayPlaying] = useState(false);
  const [roomChecked, setRoomChecked] = useState(false);
  const replayDecorations = useRef<string[]>([]);

  const [readCodeConfirmOpen, setReadCodeConfirmOpen] = useState(false);

  const hasName = name.trim().length > 0;
  const isClosed = closedAt !== null || replayData !== undefined;

  useEffect(() => {
    setHostToken(getHostToken(id));
    setIsHost(false);
    setClosedAt(null);
    setReplayData(undefined);
    setReplayPositionMs(0);
    setReplayPlaying(false);
    setRoomChecked(false);
    setConnection("disconnected");
  }, [id]);

  useEffect(() => {
    let cancelled = false;
    async function checkRoom() {
      try {
        const response = await fetch(getReplayUri(id));
        const replay = (await response.json()) as ReplayResponse;
        if (cancelled) return;
        if (replay.closed_at !== null) {
          setClosedAt(replay.closed_at);
          setConnection("closed");
          setReplayData(replay);
          setReplayPositionMs(0);
          setReplayPlaying(false);
        }
      } finally {
        if (!cancelled) setRoomChecked(true);
      }
    }
    checkRoom();
    return () => {
      cancelled = true;
    };
  }, [id]);

  useEffect(() => {
    const current = vimModeRef.current;
    current?.dispose();
    vimModeRef.current = undefined;

    if (!editor || !vimMode || isClosed || !vimStatusRef.current) return;

    let disposed = false;
    let instance: VimAdapterInstance | undefined;
    const statusNode = vimStatusRef.current;
    import("monaco-vim").then(({ initVimMode }) => {
      if (disposed) return;
      instance = initVimMode(editor, statusNode);
      vimModeRef.current = instance;
      editor.focus();
    });

    return () => {
      disposed = true;
      instance?.dispose();
      if (vimModeRef.current === instance) {
        vimModeRef.current = undefined;
      }
    };
  }, [editor, vimMode, isClosed]);

  useEffect(() => {
    if (editor?.getModel() && hasName && !replayData) {
      const model = editor.getModel()!;
      model.setValue("");
      model.setEOL(0); // LF
      const client = new Rustpad({
        uri: getWsUri(id),
        editor,
        userInfo: { name: name.trim(), hue },
        hostToken,
        onConnected: () => setConnection("connected"),
        onDisconnected: () =>
          setConnection((current) =>
            current === "closed" ? "closed" : "disconnected",
          ),
        onDesynchronized: () => {
          setConnection("desynchronized");
          toast({
            title: "Desynchronized with server",
            description: "Please save your work and refresh the page.",
            status: "error",
            duration: null,
          });
        },
        onChangeLanguage: (language) => {
          if (languages.includes(language)) {
            setLanguage(language);
          }
        },
        onChangeUsers: setUsers,
        onHostToken: (token) => {
          localStorage.setItem(`hostToken:${id}`, token);
          setHostToken(token);
        },
        onRoomState: (state) => {
          setIsHost(state.is_host);
          setClosedAt(state.closed_at);
          if (state.closed_at !== null) {
            setConnection("closed");
            loadReplay();
          }
        },
        onRoomClosed: (closedAt) => {
          setClosedAt(closedAt);
          setConnection("closed");
          loadReplay();
        },
      });
      rustpad.current = client;
      return () => {
        client.dispose();
        if (rustpad.current === client) {
          rustpad.current = undefined;
        }
      };
    }
  }, [id, editor, toast, setUsers, hasName, hostToken, replayData]);

  useEffect(() => {
    if (connection === "connected" && !closedAt) {
      rustpad.current?.setInfo({ name, hue });
    }
  }, [connection, name, hue, closedAt]);

  useEffect(() => {
    if (!editor?.getModel() || !replayData) return;
    const model = editor.getModel()!;
    const frame = buildReplayFrame(replayData, replayPositionMs);
    if (model.getValue() !== frame.text) {
      model.setValue(frame.text);
    }
    if (frame.language && frame.language !== language) {
      setLanguage(frame.language);
    }
    replayDecorations.current = renderReplayDecorations(
      model,
      replayDecorations.current,
      frame.users,
      frame.cursors,
      frame.cursorActivity,
      replayPositionMs,
    );
  }, [editor, replayData, replayPositionMs, language]);

  useEffect(() => {
    if (!replayPlaying || !replayData) return;
    const duration = replayDuration(replayData);
    const interval = window.setInterval(() => {
      setReplayPositionMs((position) => {
        const next = Math.min(position + 100 * replaySpeed, duration);
        if (next >= duration) setReplayPlaying(false);
        return next;
      });
    }, 100);
    return () => window.clearInterval(interval);
  }, [replayPlaying, replayData, replaySpeed]);

  async function loadReplay() {
    rustpad.current?.dispose();
    rustpad.current = undefined;
    const response = await fetch(getReplayUri(id));
    const replay = (await response.json()) as ReplayResponse;
    setReplayData(replay);
    setReplayPositionMs(0);
    setReplayPlaying(false);
    setRoomChecked(true);
  }

  function handleLanguageChange(language: string) {
    if (closedAt !== null || replayData) return;
    setLanguage(language);
    if (rustpad.current?.setLanguage(language)) {
      toast({
        title: "Language updated",
        description: (
          <>
            All users are now editing in{" "}
            <Text as="span" fontWeight="semibold">
              {language}
            </Text>
            .
          </>
        ),
        status: "info",
        duration: 2000,
        isClosable: true,
      });
    }
  }

  function handleLoadSample(confirmed: boolean) {
    if (closedAt !== null || replayData) return;
    if (editor?.getModel()) {
      const model = editor.getModel()!;
      const range = model.getFullModelRange();

      // If there are at least 10 lines of code, ask for confirmation.
      if (range.endLineNumber >= 10 && !confirmed) {
        setReadCodeConfirmOpen(true);
        return;
      }

      model.pushEditOperations(
        editor.getSelections(),
        [{ range, text: rustpadRaw }],
        () => null,
      );
      editor.setPosition({ column: 0, lineNumber: 0 });
      if (language !== "rust") {
        handleLanguageChange("rust");
      }
    }
  }

  function handleDarkModeChange() {
    setDarkMode(!darkMode);
  }

  function handleVimModeChange() {
    setVimMode(!vimMode);
  }

  function handleStopRoom() {
    rustpad.current?.stopRoom(hostToken);
  }

  if (!hasName && !roomChecked && !replayData) {
    return (
      <Box
        minH="100vh"
        bgColor={darkMode ? "#1e1e1e" : "white"}
        color={darkMode ? "#cbcaca" : "inherit"}
        display="grid"
        placeItems="center"
      >
        Loading room...
      </Box>
    );
  }

  if (!hasName && !replayData) {
    return <NameGate darkMode={darkMode} onSubmit={setName} />;
  }

  const replayDurationMs = replayData ? replayDuration(replayData) : 0;
  const replayActivityBuckets = replayData
    ? replayActivity(replayData, 96)
    : [];

  return (
    <Flex
      direction="column"
      h="100vh"
      overflow="hidden"
      bgColor={darkMode ? "#1e1e1e" : "white"}
      color={darkMode ? "#cbcaca" : "inherit"}
    >
      <Box
        flexShrink={0}
        bgColor={darkMode ? "#333333" : "#e8e8e8"}
        color={darkMode ? "#cccccc" : "#383838"}
        textAlign="center"
        fontSize="sm"
        py={0.5}
      >
        Rustpad
      </Box>
      <Flex flex="1 0" minH={0}>
        <Sidebar
          documentId={id}
          connection={connection}
          darkMode={darkMode}
          language={language}
          currentUser={{ name: hasName ? name.trim() : "Replay viewer", hue }}
          users={users}
          onDarkModeChange={handleDarkModeChange}
          onLanguageChange={handleLanguageChange}
          onLoadSample={() => handleLoadSample(false)}
          onChangeName={(name) => name.length > 0 && setName(name)}
          onChangeColor={() => setHue(generateHue())}
          isHost={isHost}
          isClosed={isClosed}
          vimMode={vimMode}
          onStopRoom={handleStopRoom}
          onVimModeChange={handleVimModeChange}
        />
        <ReadCodeConfirm
          isOpen={readCodeConfirmOpen}
          onClose={() => setReadCodeConfirmOpen(false)}
          onConfirm={() => {
            handleLoadSample(true);
            setReadCodeConfirmOpen(false);
          }}
        />

        <Flex flex={1} minW={0} h="100%" direction="column" overflow="hidden">
          <HStack
            h={6}
            spacing={1}
            color="#888888"
            fontWeight="medium"
            fontSize="13px"
            px={3.5}
            flexShrink={0}
          >
            <Icon as={VscFolderOpened} fontSize="md" color="blue.500" />
            <Text>documents</Text>
            <Icon as={VscChevronRight} fontSize="md" />
            <Icon as={VscGist} fontSize="md" color="purple.500" />
            <Text>{id}</Text>
            <Box flex={1} />
            <Box
              ref={vimStatusRef}
              minW="7rem"
              textAlign="right"
              color={vimMode && !isClosed ? "green.500" : "gray.500"}
              fontFamily="mono"
            />
          </HStack>
          <Box flex={1} minH={0}>
            <Editor
              theme={darkMode ? "vs-dark" : "vs"}
              language={language}
              options={{
                automaticLayout: true,
                fontSize: 13,
                readOnly: isClosed,
              }}
              onMount={(editor) => setEditor(editor)}
            />
          </Box>
          {replayData && (
            <ReplayControls
              isPlaying={replayPlaying}
              positionMs={replayPositionMs}
              durationMs={replayDurationMs}
              speed={replaySpeed}
              activityBuckets={replayActivityBuckets}
              darkMode={darkMode}
              onPlayPause={() => setReplayPlaying((playing) => !playing)}
              onPositionChange={(position) => {
                setReplayPlaying(false);
                setReplayPositionMs(position);
              }}
              onSpeedChange={setReplaySpeed}
            />
          )}
        </Flex>
      </Flex>
      <Footer />
    </Flex>
  );
}

type ReplayResponse = {
  id: string;
  created_at: number;
  closed_at: number | null;
  language: string | null;
  final_text: string;
  events: ReplayEvent[];
};

type ReplayEvent =
  | { type: "UserInfo"; at_ms: number; id: number; info: UserInfo | null }
  | { type: "Edit"; at_ms: number; id: number; operation: any }
  | { type: "Cursor"; at_ms: number; id: number; data: CursorData }
  | { type: "Language"; at_ms: number; language: string }
  | { type: "Closed"; at_ms: number };

type ReplayFrame = {
  text: string;
  language?: string;
  users: Record<number, UserInfo>;
  cursors: Record<number, CursorData>;
  cursorActivity: Record<number, number>;
};

function replayDuration(replay: ReplayResponse) {
  return replay.events.reduce((max, event) => Math.max(max, event.at_ms), 0);
}

function replayActivity(replay: ReplayResponse, bucketCount: number) {
  const duration = Math.max(replayDuration(replay), 1);
  const buckets = Array.from({ length: bucketCount }, () => 0);
  for (const event of replay.events) {
    const index = Math.min(
      bucketCount - 1,
      Math.floor((event.at_ms / duration) * bucketCount),
    );
    buckets[index] += 1;
  }
  return buckets;
}

function buildReplayFrame(
  replay: ReplayResponse,
  positionMs: number,
): ReplayFrame {
  let text = "";
  let language = replay.language ?? undefined;
  const users: Record<number, UserInfo> = {};
  const cursors: Record<number, CursorData> = {};
  const cursorActivity: Record<number, number> = {};

  for (const event of replay.events) {
    if (event.at_ms > positionMs) break;
    if (event.type === "UserInfo") {
      if (event.info) {
        users[event.id] = event.info;
      } else {
        delete users[event.id];
        delete cursors[event.id];
        delete cursorActivity[event.id];
      }
    } else if (event.type === "Edit") {
      cursorActivity[event.id] = event.at_ms;
      const operation = OpSeq.from_str(JSON.stringify(event.operation));
      if (operation) {
        text = operation.apply(text) ?? text;
        for (const data of Object.values(cursors)) {
          data.cursors = data.cursors.map((cursor) =>
            operation.transform_index(cursor),
          );
          data.selections = data.selections.map(([start, end]) => [
            operation.transform_index(start),
            operation.transform_index(end),
          ]);
        }
      }
    } else if (event.type === "Cursor") {
      cursors[event.id] = {
        cursors: [...event.data.cursors],
        selections: event.data.selections.map(([start, end]) => [start, end]),
      };
      cursorActivity[event.id] = event.at_ms;
    } else if (event.type === "Language") {
      language = event.language;
    }
  }

  return { text, language, users, cursors, cursorActivity };
}

function renderReplayDecorations(
  model: editor.ITextModel,
  oldDecorations: string[],
  users: Record<number, UserInfo>,
  cursors: Record<number, CursorData>,
  cursorActivity: Record<number, number>,
  positionMs: number,
) {
  const decorations: editor.IModelDeltaDecoration[] = [];
  for (const [id, data] of Object.entries(cursors)) {
    const user = users[Number(id)];
    if (!user) continue;
    generateReplayCursorStyles(id, user);
    const active =
      positionMs - (cursorActivity[Number(id)] ?? 0) < CURSOR_LABEL_VISIBLE_MS;
    for (const cursor of data.cursors) {
      const position = unicodePosition(model, cursor);
      const labelPosition =
        position.lineNumber === 1
          ? "replay-cursor-label-below"
          : "replay-cursor-label-above";
      decorations.push({
        options: {
          className: `replay-cursor-${id}`,
          afterContentClassName: `replay-cursor-label-${id} ${
            active ? labelPosition : "replay-cursor-label-hidden"
          }`,
          stickiness: 1,
          zIndex: 2,
        },
        range: {
          startLineNumber: position.lineNumber,
          startColumn: position.column,
          endLineNumber: position.lineNumber,
          endColumn: position.column,
        },
      });
    }
    for (const [start, end] of data.selections) {
      const position = unicodePosition(model, start);
      const positionEnd = unicodePosition(model, end);
      decorations.push({
        options: {
          className: `replay-selection-${id}`,
          hoverMessage: { value: user.name },
          stickiness: 1,
          zIndex: 1,
        },
        range: {
          startLineNumber: position.lineNumber,
          startColumn: position.column,
          endLineNumber: positionEnd.lineNumber,
          endColumn: positionEnd.column,
        },
      });
    }
  }
  return model.deltaDecorations(oldDecorations, decorations);
}

function unicodePosition(model: editor.ITextModel, offset: number) {
  const value = model.getValue();
  let offsetUTF16 = 0;
  for (const char of value) {
    if (offset <= 0) break;
    offsetUTF16 += char.length;
    offset -= 1;
  }
  return model.getPositionAt(offsetUTF16);
}

const generatedReplayStyles = new Map<string, string>();

function generateReplayCursorStyles(id: string, user: UserInfo) {
  const key = `${user.hue}:${user.name}`;
  if (generatedReplayStyles.get(id) === key) return;
  generatedReplayStyles.set(id, key);
  const css = `
    .monaco-editor .replay-selection-${id} {
      background-color: hsla(${user.hue}, 90%, 80%, 0.5);
    }
    .monaco-editor .replay-cursor-${id} {
      border-left: 2px solid hsl(${user.hue}, 90%, 25%);
    }
    .monaco-editor .replay-cursor-label-${id}::after {
      content: ${JSON.stringify(user.name)};
      position: absolute;
      background: hsl(${user.hue}, 80%, 35%);
      color: white;
      border-radius: 3px;
      font-size: 11px;
      line-height: 14px;
      padding: 0 4px;
      pointer-events: none;
      white-space: nowrap;
      z-index: 10;
    }
    .monaco-editor .replay-cursor-label-${id}.replay-cursor-label-above::after {
      transform: translate(2px, -1.35em);
    }
    .monaco-editor .replay-cursor-label-${id}.replay-cursor-label-below::after {
      transform: translate(2px, 0.2em);
    }
    .monaco-editor .replay-cursor-label-${id}.replay-cursor-label-hidden::after {
      display: none;
    }
  `;
  const element = document.createElement("style");
  element.appendChild(document.createTextNode(css));
  document.head.appendChild(element);
}

export default App;
