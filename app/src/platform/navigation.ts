import type { IconName } from "../components/Icon";
import type { PageId } from "../model";

export interface NavigationItem {
  id: PageId;
  label: string;
  icon: IconName;
}
