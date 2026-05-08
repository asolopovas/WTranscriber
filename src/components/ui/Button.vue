<script setup lang="ts">
import { computed } from "vue";
import Icon from "@components/ui/Icon.vue";

type Variant =
  | "primary"
  | "neutral"
  | "danger"
  | "error"
  | "ghost"
  | "ghost-primary"
  | "ghost-error";

type Shape = "pill" | "square" | "circle" | "icon" | "tab" | "tab-mobile" | "link";

type Size = "sm" | "md" | "lg";

const props = withDefaults(
  defineProps<{
    variant?: Variant;
    shape?: Shape;
    size?: Size;
    icon?: string;
    iconSize?: number;
    iconFill?: boolean;
    block?: boolean;
    mobileTall?: boolean;
    active?: boolean;
    bold?: boolean;
    type?: "button" | "submit" | "reset";
  }>(),
  {
    variant: "neutral",
    shape: "pill",
    size: "sm",
    icon: undefined,
    iconSize: 16,
    iconFill: false,
    block: false,
    mobileTall: false,
    active: false,
    bold: false,
    type: "button",
  },
);

const TONE: Record<Variant, string> = {
  primary: "bg-primary text-on-primary hover:bg-primary-fixed-dim",
  neutral: "border border-outline-variant text-on-surface hover:bg-surface-container-high",
  danger: "border border-error/60 text-error hover:bg-error/10",
  error: "bg-error-container text-on-error-container hover:opacity-90",
  ghost: "text-on-surface-variant hover:text-on-surface hover:bg-surface-container-highest",
  "ghost-primary": "text-primary hover:bg-surface-container-highest",
  "ghost-error": "text-error hover:bg-error-container/40",
};

const PILL_SIZE: Record<Size, string> = {
  sm: "px-md py-xs text-titleSmall",
  md: "min-h-9 px-md text-titleSmall",
  lg: "min-h-12 px-lg text-titleSmall",
};

const SQUARE_SIZE: Record<Size, string> = {
  sm: "h-8 px-md text-labelLarge",
  md: "h-9 px-md text-labelLarge",
  lg: "h-12 px-lg text-titleSmall",
};

const CIRCLE_SIZE: Record<Size, string> = {
  sm: "w-9 h-9",
  md: "w-11 h-11",
  lg: "w-12 h-12",
};

const SHAPE_BASE =
  "inline-flex items-center justify-center gap-unit transition-colors disabled:opacity-50 disabled:cursor-not-allowed";

function shapeClasses(shape: Shape, size: Size): string {
  switch (shape) {
    case "pill":
      return `${SHAPE_BASE} rounded-full ${PILL_SIZE[size]}`;
    case "square":
      return `${SHAPE_BASE} rounded-md ${SQUARE_SIZE[size]}`;
    case "circle":
      return `${SHAPE_BASE} rounded-full ${CIRCLE_SIZE[size]}`;
    case "icon":
      return `${SHAPE_BASE} p-unit rounded`;
    case "tab":
      return "h-full inline-flex items-center text-titleSmall border-b-2 px-unit transition-colors whitespace-nowrap shrink-0";
    case "tab-mobile":
      return "flex-1 inline-flex flex-col items-center justify-center h-14 transition-colors";
    case "link":
      return "text-titleSmall underline hover:opacity-80 shrink-0 transition-opacity";
  }
}

const TAB_ACTIVE: Record<"tab" | "tab-mobile", { on: string; off: string }> = {
  tab: {
    on: "border-primary text-on-surface",
    off: "border-transparent text-on-surface-variant hover:text-on-surface",
  },
  "tab-mobile": {
    on: "text-primary",
    off: "text-on-surface-variant hover:text-on-surface",
  },
};

const classes = computed(() => {
  const parts: string[] = [shapeClasses(props.shape, props.size)];

  if (props.shape === "tab" || props.shape === "tab-mobile") {
    parts.push(props.active ? TAB_ACTIVE[props.shape].on : TAB_ACTIVE[props.shape].off);
  } else if (props.shape !== "link") {
    parts.push(TONE[props.variant]);
  }

  if (props.bold) parts.push("font-bold");
  if (props.block) parts.push("flex-1 w-full");
  if (props.mobileTall) parts.push("min-h-11 md:min-h-0");

  return parts.filter(Boolean).join(" ");
});
</script>

<template>
  <button :type="type" :class="classes">
    <Icon v-if="icon" :name="icon" :size="iconSize" :fill="iconFill" />
    <slot />
  </button>
</template>
