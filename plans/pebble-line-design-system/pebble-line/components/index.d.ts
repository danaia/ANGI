import type * as React from 'react';

/** Names of the built-in 24px stroke icons. */
export type IconName = 'user' | 'lock' | 'mail' | 'search' | 'eye' | 'eyeOff' | 'home' | 'doc' | 'cart' | 'chart' | 'dollar' | 'sliders' | 'bell' | 'check' | 'x' | 'plus' | 'minus' | 'chevronDown' | 'chevronRight' | 'chevronLeft' | 'arrowUp' | 'arrowDown' | 'info' | 'alert' | 'checkCircle' | 'calendar' | 'grid' | 'edit' | 'trash' | 'logout' | 'more' | 'heart' | 'settings';
export type Tone = 'neutral' | 'success' | 'warning' | 'danger';
type Option<V = string> = V | { value: V; label: React.ReactNode; disabled?: boolean; icon?: IconName };

export interface IconProps { name: IconName; size?: number; strokeWidth?: number; label?: string; className?: string }
export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> { variant?: 'secondary' | 'primary' | 'ghost' | 'danger'; size?: 'sm' | 'md' | 'lg'; icon?: IconName; iconRight?: IconName; block?: boolean }
export interface IconButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> { icon: IconName; label: string; size?: 'sm' | 'md' | 'lg'; shape?: 'round' | 'square'; active?: boolean }
export interface TextFieldProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'prefix'> { label?: React.ReactNode; hint?: React.ReactNode; error?: React.ReactNode; icon?: IconName; prefix?: React.ReactNode }
export interface TextAreaProps extends React.TextareaHTMLAttributes<HTMLTextAreaElement> { label?: React.ReactNode; hint?: React.ReactNode; error?: React.ReactNode }
export interface SelectProps extends React.SelectHTMLAttributes<HTMLSelectElement> { label?: React.ReactNode; hint?: React.ReactNode; error?: React.ReactNode; options: Option[]; placeholder?: string }
export interface CheckboxProps extends React.InputHTMLAttributes<HTMLInputElement> { label?: React.ReactNode }
export interface RadioGroupProps { label?: React.ReactNode; options: Option[]; value?: string; defaultValue?: string; onChange?: (value: string) => void; name?: string; disabled?: boolean; className?: string }
export interface SwitchProps { label?: React.ReactNode; checked?: boolean; defaultChecked?: boolean; onChange?: (checked: boolean) => void; disabled?: boolean; id?: string; 'aria-label'?: string; className?: string }
export interface SliderProps { label?: React.ReactNode; min?: number; max?: number; step?: number; value?: number; defaultValue?: number; onChange?: (value: number) => void; format?: (value: number) => string; showValue?: boolean; disabled?: boolean; 'aria-label'?: string; className?: string }
export interface RangeSliderProps { label?: string; min?: number; max?: number; step?: number; value?: [number, number]; defaultValue?: [number, number]; onChange?: (value: [number, number]) => void; format?: (value: number) => string; minLabel?: string; maxLabel?: string; showValues?: boolean; className?: string }
export interface SegmentedControlProps { options: Option[]; value?: string; defaultValue?: string; onChange?: (value: string) => void; 'aria-label'?: string; className?: string }
export interface TabsProps { items: { id: string; label: React.ReactNode; content?: React.ReactNode }[]; value?: string; defaultValue?: string; onChange?: (id: string) => void; 'aria-label'?: string; className?: string }
export interface BreadcrumbsProps { items: { label: React.ReactNode; href?: string }[]; className?: string }
export interface PaginationProps { pageCount: number; page?: number; defaultPage?: number; onChange?: (page: number) => void; className?: string }
export interface NavItem { id: string; label: string; icon?: IconName; href?: string; badge?: React.ReactNode }
export interface NavListProps { items: NavItem[]; value?: string; defaultValue?: string; onChange?: (id: string) => void; 'aria-label'?: string; className?: string }
export interface NavRailProps { items: (NavItem & { icon: IconName })[]; value?: string; defaultValue?: string; onChange?: (id: string) => void; 'aria-label'?: string; className?: string }
export interface CardProps extends Omit<React.HTMLAttributes<HTMLElement>, 'title'> { variant?: 'outline' | 'hero' | 'muted' | 'accent'; eyebrow?: React.ReactNode; title?: React.ReactNode; actions?: React.ReactNode; as?: keyof JSX.IntrinsicElements }
export interface DividerProps { variant?: 'hairline' | 'rule'; className?: string }
export interface BentoGridProps { columns?: number; rowHeight?: number; style?: React.CSSProperties; className?: string; children?: React.ReactNode }
export interface BentoCellProps { cols?: number; rows?: number; className?: string; children?: React.ReactNode }
export interface StatTileProps { label: React.ReactNode; value: React.ReactNode; icon?: IconName; delta?: React.ReactNode; trend?: 'up' | 'down'; variant?: 'outline' | 'muted' | 'accent'; style?: React.CSSProperties; className?: string }
export interface SummaryListProps { rows: { label: React.ReactNode; value: React.ReactNode; total?: boolean }[]; className?: string }
export interface DialogProps { open?: boolean; title: React.ReactNode; onClose?: () => void; actions?: React.ReactNode; inline?: boolean; className?: string; children?: React.ReactNode }
export interface MenuItem { label?: React.ReactNode; icon?: IconName; shortcut?: string; danger?: boolean; separator?: boolean; onSelect?: () => void }
export interface MenuProps { items: MenuItem[]; onSelect?: (item: MenuItem) => void; 'aria-label'?: string; className?: string }
export interface TooltipProps { content: React.ReactNode; open?: boolean; children: React.ReactElement }
export interface AlertProps { tone?: Tone; title?: React.ReactNode; icon?: IconName; onClose?: () => void; className?: string; children?: React.ReactNode }
export interface ToastProps { tone?: Tone; icon?: IconName; action?: React.ReactNode; onAction?: () => void; onClose?: () => void; className?: string; children?: React.ReactNode }
export interface BadgeProps { tone?: Tone | 'accent'; dot?: boolean; className?: string; children?: React.ReactNode }
export interface ProgressProps { value: number; max?: number; label?: string; showValue?: boolean; 'aria-label'?: string; className?: string }
export interface ProgressRingProps { value: number; max?: number; size?: number; thickness?: number; label?: string; 'aria-label'?: string; className?: string; children?: React.ReactNode }
export interface AvatarProps { name: string; src?: string; size?: number; status?: 'online' | 'away' | 'offline'; className?: string }
export interface TableColumn<R = any> { key: string; label: React.ReactNode; align?: 'left' | 'right'; render?: (row: R) => React.ReactNode }
export interface TableProps<R = any> { columns: TableColumn<R>[]; rows: R[]; caption?: string; style?: React.CSSProperties; className?: string }

export declare function Icon(props: IconProps): React.ReactElement;
export declare function Button(props: ButtonProps): React.ReactElement;
export declare function IconButton(props: IconButtonProps): React.ReactElement;
export declare function TextField(props: TextFieldProps): React.ReactElement;
export declare function TextArea(props: TextAreaProps): React.ReactElement;
export declare function Select(props: SelectProps): React.ReactElement;
export declare function Checkbox(props: CheckboxProps): React.ReactElement;
export declare function RadioGroup(props: RadioGroupProps): React.ReactElement;
export declare function Switch(props: SwitchProps): React.ReactElement;
export declare function Slider(props: SliderProps): React.ReactElement;
export declare function RangeSlider(props: RangeSliderProps): React.ReactElement;
export declare function SegmentedControl(props: SegmentedControlProps): React.ReactElement;
export declare function Tabs(props: TabsProps): React.ReactElement;
export declare function Breadcrumbs(props: BreadcrumbsProps): React.ReactElement;
export declare function Pagination(props: PaginationProps): React.ReactElement;
export declare function NavList(props: NavListProps): React.ReactElement;
export declare function NavRail(props: NavRailProps): React.ReactElement;
export declare function Card(props: CardProps): React.ReactElement;
export declare function Divider(props: DividerProps): React.ReactElement;
export declare function BentoGrid(props: BentoGridProps): React.ReactElement;
export declare function BentoCell(props: BentoCellProps): React.ReactElement;
export declare function StatTile(props: StatTileProps): React.ReactElement;
export declare function SummaryList(props: SummaryListProps): React.ReactElement;
export declare function Dialog(props: DialogProps): React.ReactElement | null;
export declare function Menu(props: MenuProps): React.ReactElement;
export declare function Tooltip(props: TooltipProps): React.ReactElement;
export declare function Alert(props: AlertProps): React.ReactElement;
export declare function Toast(props: ToastProps): React.ReactElement;
export declare function Badge(props: BadgeProps): React.ReactElement;
export declare function Progress(props: ProgressProps): React.ReactElement;
export declare function ProgressRing(props: ProgressRingProps): React.ReactElement;
export declare function Avatar(props: AvatarProps): React.ReactElement;
export declare function Table<R = any>(props: TableProps<R>): React.ReactElement;
